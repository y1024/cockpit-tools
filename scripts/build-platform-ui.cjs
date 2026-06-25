#!/usr/bin/env node

const fs = require('node:fs');
const path = require('node:path');
const postcss = require('postcss');
const selectorParser = require('postcss-selector-parser');

const ROOT = path.resolve(__dirname, '..');
const SUPPORTED_PLATFORMS = new Set(['antigravity', 'antigravity_ide', 'zed', 'kiro', 'github-copilot', 'windsurf', 'cursor', 'gemini', 'trae', 'qoder', 'codebuddy', 'codebuddy_cn', 'workbuddy', 'claude_manager', 'codex']);

function fail(message) {
  console.error(message);
  process.exit(1);
}

function assertRemoteExport(source, exportName) {
  const escapedName = exportName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const directExport = new RegExp(
    `export\\s+(?:async\\s+)?function\\s+${escapedName}\\b|export\\s+(?:const|let|var)\\s+${escapedName}\\b`,
  ).test(source);
  const namedExport = new RegExp(
    `export\\s*\\{[\\s\\S]*?(?:\\b${escapedName}\\b|\\bas\\s+${escapedName}\\b)[\\s\\S]*?\\}`,
  ).test(source);
  if (!directExport && !namedExport) {
    fail(`Platform remote UI is missing ${exportName} export`);
  }
}

function assertBrowserRuntimeSource(source) {
  if (/\bprocess\s*\.\s*env\b/.test(source)) {
    fail('Platform remote UI contains process.env reference; browser runtime has no Node process global');
  }
}

function stripTrailingWhitespace(source) {
  return source.replace(/[ \t]+$/gm, '');
}

function platformRootClassName(platformId) {
  return `${platformId}-platform-ui-root`;
}

function isKeyframesRule(rule) {
  let parent = rule.parent;
  while (parent) {
    if (parent.type === 'atrule' && /keyframes$/i.test(parent.name)) {
      return true;
    }
    parent = parent.parent;
  }
  return false;
}

function hasRootClass(selector, rootClassName) {
  let matched = false;
  selector.walkClasses((node) => {
    if (node.value === rootClassName) {
      matched = true;
    }
  });
  return matched;
}

function isThemeAttribute(node) {
  return node.type === 'attribute'
    && String(node.attribute || '').toLowerCase() === 'data-theme';
}

function themePrefixInsertIndex(selector) {
  if (!selector.nodes.length || !isThemeAttribute(selector.nodes[0])) {
    return 0;
  }
  return selector.nodes[1]?.type === 'combinator' ? 2 : 1;
}

function prefixSelector(selector, rootClassName) {
  if (hasRootClass(selector, rootClassName)) {
    return;
  }

  const rootNode = selectorParser.className({ value: rootClassName });
  const spaceNode = selectorParser.combinator({ value: ' ' });
  const insertIndex = themePrefixInsertIndex(selector);
  const target = selector.at(insertIndex);
  if (target) {
    selector.insertBefore(target, rootNode);
    selector.insertBefore(target, spaceNode);
    return;
  }

  const last = selector.nodes[selector.nodes.length - 1];
  if (last && last.type !== 'combinator') {
    selector.append(selectorParser.combinator({ value: ' ' }));
  }
  selector.append(rootNode);
}

function assertNoForbiddenGlobalSelectors(source) {
  const root = postcss.parse(source);
  root.walkRules((rule) => {
    if (isKeyframesRule(rule)) {
      return;
    }
    selectorParser((selectors) => {
      selectors.each((selector) => {
        selector.walk((node) => {
          if (node.type === 'tag' && ['html', 'body'].includes(node.value.toLowerCase())) {
            fail(`Platform remote UI style contains forbidden global selector "${rule.selector}"`);
          }
          if (node.type === 'id' && node.value === 'root') {
            fail(`Platform remote UI style contains forbidden global selector "${rule.selector}"`);
          }
          if (node.type === 'pseudo' && node.value === ':root') {
            fail(`Platform remote UI style contains forbidden global selector "${rule.selector}"`);
          }
          if (node.type === 'universal') {
            fail(`Platform remote UI style contains forbidden global selector "${rule.selector}"`);
          }
        });
      });
    }).processSync(rule.selector);
  });
}

function assertScopedRemoteStyle(platformId, source) {
  const rootClassName = platformRootClassName(platformId);
  assertNoForbiddenGlobalSelectors(source);
  const root = postcss.parse(source);
  root.walkRules((rule) => {
    if (isKeyframesRule(rule)) {
      return;
    }
    selectorParser((selectors) => {
      selectors.each((selector) => {
        if (!hasRootClass(selector, rootClassName)) {
          fail(
            `Platform remote UI style selector is not scoped to .${rootClassName}: "${selector.toString()}"`,
          );
        }
      });
    }).processSync(rule.selector);
  });
}

function scopeRemoteStyle(platformId, source) {
  const rootClassName = platformRootClassName(platformId);
  assertNoForbiddenGlobalSelectors(source);
  const root = postcss.parse(source);
  root.walkRules((rule) => {
    if (isKeyframesRule(rule)) {
      return;
    }
    rule.selector = selectorParser((selectors) => {
      selectors.each((selector) => prefixSelector(selector, rootClassName));
    }).processSync(rule.selector);
  });
  const scoped = root.toString();
  assertScopedRemoteStyle(platformId, scoped);
  return scoped;
}

async function main() {
  const platformId = (process.argv[2] || '').trim();
  if (!SUPPORTED_PLATFORMS.has(platformId)) {
    fail(`Usage: node scripts/build-platform-ui.cjs <${Array.from(SUPPORTED_PLATFORMS).join('|')}>`);
  }

  const { build } = await import('vite');
  const react = (await import('@vitejs/plugin-react')).default;
  const tempRoot = fs.mkdtempSync(path.join(ROOT, `.tmp-${platformId}-ui-`));
  const outDir = path.join(tempRoot, 'dist');
  const entry = path.join(ROOT, 'src', 'platform-ui', platformId, 'remote.tsx');
  const targetDir = path.join(ROOT, 'platform-packages', platformId, 'ui');

  try {
    await build({
      root: ROOT,
      configFile: false,
      publicDir: false,
      logLevel: 'warn',
      plugins: [react()],
      define: {
        'process.env.NODE_ENV': JSON.stringify('production'),
      },
      resolve: {
        alias: [
          { find: '@', replacement: path.join(ROOT, 'src') },
          { find: 'react/jsx-runtime', replacement: require.resolve('react/jsx-runtime') },
          { find: 'react/jsx-dev-runtime', replacement: require.resolve('react/jsx-dev-runtime') },
          { find: 'react', replacement: require.resolve('react') },
          { find: 'react-dom/client', replacement: require.resolve('react-dom/client') },
          { find: 'react-dom', replacement: require.resolve('react-dom') },
          { find: 'lucide-react', replacement: require.resolve('lucide-react') },
        ],
      },
      build: {
        outDir,
        emptyOutDir: true,
        target: 'es2020',
        cssCodeSplit: false,
        lib: {
          entry,
          formats: ['es'],
          fileName: () => 'remoteEntry.js',
        },
        rollupOptions: {
          preserveEntrySignatures: 'strict',
          output: {
            format: 'es',
            inlineDynamicImports: true,
            assetFileNames: (assetInfo) => {
              if (assetInfo.name && assetInfo.name.endsWith('.css')) {
                return 'style.css';
              }
              return '[name][extname]';
            },
          },
        },
      },
    });

    const remoteEntry = path.join(outDir, 'remoteEntry.js');
    const style = path.join(outDir, 'style.css');
    if (!fs.existsSync(remoteEntry)) {
      fail(`Failed to locate remoteEntry.js for ${platformId}`);
    }
    const remoteSource = stripTrailingWhitespace(fs.readFileSync(remoteEntry, 'utf8'));
    assertRemoteExport(remoteSource, 'mount');
    assertRemoteExport(remoteSource, 'unmount');
    assertBrowserRuntimeSource(remoteSource);

    fs.rmSync(targetDir, { recursive: true, force: true });
    fs.mkdirSync(targetDir, { recursive: true });
    fs.writeFileSync(path.join(targetDir, 'remoteEntry.js'), remoteSource);
    if (fs.existsSync(style)) {
      const scopedStyle = stripTrailingWhitespace(
        scopeRemoteStyle(platformId, fs.readFileSync(style, 'utf8')),
      );
      fs.writeFileSync(path.join(targetDir, 'style.css'), scopedStyle);
    } else {
      fs.writeFileSync(path.join(targetDir, 'style.css'), '');
    }
    console.log(`Built ${platformId} platform UI -> ${path.relative(ROOT, targetDir)}`);
  } finally {
    fs.rmSync(tempRoot, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
