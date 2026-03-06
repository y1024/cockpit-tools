#!/usr/bin/env node

/**
 * 翻译文件检查脚本
 * - 检测不同语言翻译文件之间的 key 差异
 * - 检测非英语语言是否复用英文值（同 key）
 */

const fs = require('fs');
const path = require('path');

// 配置
const LOCALES_DIR = path.join(__dirname, '../src/locales');
const BASELINE_FILE = 'en-US.json'; // 基准文件
const ENGLISH_REFERENCE_FILES = new Set(['en-US.json', 'en.json']);
const MIN_REUSED_ENGLISH_LOCALES = 2;
const VALUE_REUSE_PREVIEW_LIMIT = 10;
const PLATFORM_COMMON_NAMESPACES = ['codex', 'githubCopilot', 'windsurf', 'kiro'];
const MIN_PLATFORM_DUPLICATE_COUNT = 2;
const PLATFORM_DUP_PREVIEW_LIMIT = 10;
const FAIL_ON_PLATFORM_COMMON_DUP = process.env.LOCALE_FAIL_ON_PLATFORM_COMMON_DUP === '1';

// 颜色输出
const colors = {
  reset: '\x1b[0m',
  bright: '\x1b[1m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
};

function log(message, color = 'reset') {
  console.log(`${colors[color]}${message}${colors.reset}`);
}

/**
 * 递归获取所有的 key 路径
 * @param {Object} obj - JSON 对象
 * @param {string} prefix - 当前路径前缀
 * @returns {Set<string>} - 所有 key 的集合
 */
function getAllKeys(obj, prefix = '') {
  const keys = new Set();
  
  if (!obj || typeof obj !== 'object') {
    return keys;
  }
  
  for (const key in obj) {
    if (!obj.hasOwnProperty(key)) continue;
    
    const fullKey = prefix ? `${prefix}.${key}` : key;
    keys.add(fullKey);
    
    if (typeof obj[key] === 'object' && !Array.isArray(obj[key])) {
      const nestedKeys = getAllKeys(obj[key], fullKey);
      nestedKeys.forEach(k => keys.add(k));
    }
  }
  
  return keys;
}

/**
 * 递归获取所有叶子字符串值（key -> value）
 * @param {Object} obj - JSON 对象
 * @param {string} prefix - 当前路径前缀
 * @returns {Map<string, string>} - 叶子字符串键值映射
 */
function getLeafStringMap(obj, prefix = '') {
  const valueMap = new Map();

  if (!obj || typeof obj !== 'object') {
    return valueMap;
  }

  for (const key in obj) {
    if (!obj.hasOwnProperty(key)) continue;

    const fullKey = prefix ? `${prefix}.${key}` : key;
    const value = obj[key];

    if (typeof value === 'string') {
      valueMap.set(fullKey, value);
      continue;
    }

    if (value && typeof value === 'object' && !Array.isArray(value)) {
      const nestedMap = getLeafStringMap(value, fullKey);
      nestedMap.forEach((nestedValue, nestedKey) => valueMap.set(nestedKey, nestedValue));
    }
  }

  return valueMap;
}

/**
 * 读取对象某个路径下的值
 * @param {Object} obj
 * @param {string} keyPath
 * @returns {any}
 */
function getValueByPath(obj, keyPath) {
  if (!obj || typeof obj !== 'object') return undefined;
  return keyPath.split('.').reduce((acc, key) => (acc == null ? undefined : acc[key]), obj);
}

/**
 * 判断字符串是否可能是英文文案
 * @param {string} value
 * @returns {boolean}
 */
function isLikelyEnglishValue(value) {
  if (typeof value !== 'string') return false;

  const normalized = value.trim();
  if (!normalized) return false;

  // 去掉插值变量，避免 {{count}} 这类占位符影响判断
  const withoutInterpolation = normalized.replace(/\{\{[^{}]+\}\}/g, '').trim();
  if (!withoutInterpolation) return false;

  // 不包含字母则不判断为英文文案
  if (!/[A-Za-z]/.test(withoutInterpolation)) return false;

  // 若包含明显的非拉丁文字符，则视为已本地化
  if (/[\u0400-\u04FF\u0600-\u06FF\u3040-\u30FF\u3400-\u9FFF\uAC00-\uD7AF]/.test(withoutInterpolation)) {
    return false;
  }

  return true;
}

/**
 * 判断该 key/value 是否属于允许保留英文的场景
 * - 品牌名、协议名、缩写
 * - 命令/路径片段、时间与占位符格式
 * - 套餐代码（如 PRO/FREE）等约定术语
 * @param {string} key
 * @param {string} value
 * @returns {boolean}
 */
function isAllowedEnglishReuse(key, value) {
  if (typeof value !== 'string') return false;

  const normalized = value.trim();
  if (!normalized) return true;

  const allowedExactValues = new Set([
    'OAuth',
    'Token / JSON',
    // 品牌名 / 产品名允许跨语言保持英文原文
    'Cockpit Tools',
    'Antigravity',
    'Codex',
    'GitHub Copilot',
    'Kiro',
    'Alipay',
    'WeChat',
    'WeChat Pay',
    'Windsurf',
    'Crontab',
    '🧩 Crontab',
    'min',
    '<1m',
    '{{ms}}ms',
    '{{days}}d',
    '{{hours}}h',
    '{{minutes}}m',
    '{{days}}d {{hours}}h',
    '{{hours}}h {{minutes}}m',
    '5h',
  ]);

  if (allowedExactValues.has(normalized)) {
    return true;
  }

  // 套餐/层级代码通常直接沿用英文缩写
  if (/(^|\.)(plan|tier|filter)\.(free|plus|pro|ultra|team|business|enterprise|individual)$/.test(key)) {
    return true;
  }

  // 命令/参数片段保留原样
  if (/restartStrategy\.force\.command(Mac|Win)$/.test(key)) {
    return true;
  }
  if (/pkill|CODEX_HOME|--user-data-dir/.test(normalized)) {
    return true;
  }

  // 唤醒日志格式字段，保留结构化英文标记
  if (/^wakeup\.format\.(durationMs|traceId|tokens|crontab)$/.test(key)) {
    return true;
  }
  if (/^wakeup\.triggerSource\.crontab$/.test(key)) {
    return true;
  }
  if (/^wakeup\.form\.modeCrontab$/.test(key)) {
    return true;
  }
  if (/^Crontab:\s*\{\{expr\}\}$/.test(normalized)) {
    return true;
  }
  if (/^traceId=\{\{traceId\}\}$/.test(normalized)) {
    return true;
  }
  if (/^tokens=\{\{prompt\}\}\+\{\{completion\}\}=\{\{total\}\}$/.test(normalized)) {
    return true;
  }

  // 少量在多语言里普遍沿用的单词/缩写，避免误报
  const allowedKeys = new Set([
    'accounts.status.normal',
    'codex.columns.plan',
    'common.shared.columns.plan',
    'settings.about.sponsor',
    'instances.labels.quota',
    'wakeup.form.intervalStart',
    'windsurf.credits.title',
    'breakout.historyLevelShort',
    'settings.general.minutes',
  ]);

  return allowedKeys.has(key);
}

/**
 * 检测平台分组下“相同语义却重复定义”的 key 候选
 * 规则：同一 suffix 在 2+ 平台命名空间中值完全一致，建议迁移到 common.*
 * @param {Object} baselineData
 * @returns {Array<{suffix: string, value: string, keys: string[], roots: string[], suggestedCommonKey: string, existingCommonKey?: string}>}
 */
function detectPlatformCommonDuplication(baselineData) {
  const valueMap = getLeafStringMap(baselineData);
  const suffixMap = new Map();

  for (const [key, value] of valueMap.entries()) {
    if (typeof value !== 'string') continue;

    const [root, ...rest] = key.split('.');
    if (!PLATFORM_COMMON_NAMESPACES.includes(root) || rest.length === 0) continue;

    const suffix = rest.join('.');
    if (!suffixMap.has(suffix)) suffixMap.set(suffix, []);
    suffixMap.get(suffix).push({ root, key, value });
  }

  const issues = [];

  for (const [suffix, entries] of suffixMap.entries()) {
    if (entries.length < MIN_PLATFORM_DUPLICATE_COUNT) continue;

    const byValue = new Map();
    for (const entry of entries) {
      const bucket = byValue.get(entry.value) || [];
      bucket.push(entry);
      byValue.set(entry.value, bucket);
    }

    for (const [value, sameValueEntries] of byValue.entries()) {
      if (sameValueEntries.length < MIN_PLATFORM_DUPLICATE_COUNT) continue;

      const roots = [...new Set(sameValueEntries.map(item => item.root))];
      if (roots.length < MIN_PLATFORM_DUPLICATE_COUNT) continue;

      const existingCommonKey = `common.${suffix}`;
      const existingCommonValue = getValueByPath(baselineData, existingCommonKey);
      const suggestedCommonKey = existingCommonValue === value
        ? existingCommonKey
        : `common.shared.${suffix}`;

      issues.push({
        suffix,
        value,
        keys: sameValueEntries.map(item => item.key).sort(),
        roots: roots.sort(),
        suggestedCommonKey,
        existingCommonKey: existingCommonValue === value ? existingCommonKey : undefined,
      });
    }
  }

  issues.sort((a, b) => {
    if (b.roots.length !== a.roots.length) return b.roots.length - a.roots.length;
    return a.suffix.localeCompare(b.suffix);
  });

  return issues;
}

/**
 * 检测非英语语言是否复用英文值（同一 key）
 * @param {string} baselineFile
 * @param {Map<string, Map<string, string>>} localeValueMaps
 * @returns {Array<{key: string, value: string, locales: string[]}>}
 */
function detectEnglishValueReuse(baselineFile, localeValueMaps) {
  const baselineValueMap = localeValueMaps.get(baselineFile);
  if (!baselineValueMap) return [];

  const nonEnglishFiles = [...localeValueMaps.keys()].filter(
    file => file !== baselineFile && !ENGLISH_REFERENCE_FILES.has(file)
  );

  const issues = [];

  for (const [key, baselineValue] of baselineValueMap.entries()) {
    if (!isLikelyEnglishValue(baselineValue)) continue;
    if (isAllowedEnglishReuse(key, baselineValue)) continue;

    const reusedLocales = [];
    for (const file of nonEnglishFiles) {
      const valueMap = localeValueMaps.get(file);
      if (!valueMap) continue;
      if (valueMap.get(key) === baselineValue) {
        reusedLocales.push(file);
      }
    }

    if (reusedLocales.length >= MIN_REUSED_ENGLISH_LOCALES) {
      issues.push({
        key,
        value: baselineValue,
        locales: reusedLocales,
      });
    }
  }

  issues.sort((a, b) => {
    if (b.locales.length !== a.locales.length) return b.locales.length - a.locales.length;
    return a.key.localeCompare(b.key);
  });

  return issues;
}

/**
 * 读取并解析 JSON 文件
 * @param {string} filePath - 文件路径
 * @returns {Object|null} - 解析后的 JSON 对象
 */
function readJsonFile(filePath) {
  try {
    const content = fs.readFileSync(filePath, 'utf8');
    return JSON.parse(content);
  } catch (error) {
    log(`错误: 无法读取文件 ${filePath}: ${error.message}`, 'red');
    return null;
  }
}

/**
 * 获取所有 locale 文件
 * @returns {Array<string>} - 文件名数组
 */
function getLocaleFiles() {
  try {
    const files = fs.readdirSync(LOCALES_DIR);
    return files.filter(file => file.endsWith('.json'));
  } catch (error) {
    log(`错误: 无法读取目录 ${LOCALES_DIR}: ${error.message}`, 'red');
    return [];
  }
}

/**
 * 主函数
 */
function main() {
  log('\n========================================', 'cyan');
  log('  翻译文件 Key 一致性检查', 'bright');
  log('========================================\n', 'cyan');
  
  // 获取所有 locale 文件
  const files = getLocaleFiles();
  if (files.length === 0) {
    log('没有找到任何翻译文件！', 'red');
    return;
  }
  
  log(`📁 找到 ${files.length} 个翻译文件:\n`, 'blue');
  files.forEach(file => log(`   - ${file}`, 'blue'));
  log('');
  
  // 读取并解析所有文件
  const localeKeys = new Map();
  const localeData = new Map();
  const localeValueMaps = new Map();
  
  for (const file of files) {
    const filePath = path.join(LOCALES_DIR, file);
    const data = readJsonFile(filePath);
    
    if (data) {
      const keys = getAllKeys(data);
      const valueMap = getLeafStringMap(data);
      localeKeys.set(file, keys);
      localeData.set(file, data);
      localeValueMaps.set(file, valueMap);
    }
  }
  
  // 统计信息
  log('========================================', 'cyan');
  log('📊 统计信息', 'bright');
  log('========================================\n', 'cyan');
  
  const stats = [];
  for (const [file, keys] of localeKeys.entries()) {
    stats.push({ file, count: keys.size });
  }
  
  // 按 key 数量排序
  stats.sort((a, b) => b.count - a.count);
  
  // 显示统计
  const maxCount = Math.max(...stats.map(s => s.count));
  const minCount = Math.min(...stats.map(s => s.count));
  
  for (const { file, count } of stats) {
    const color = count === maxCount ? 'green' : count === minCount ? 'yellow' : 'reset';
    const badge = count === maxCount ? ' [最多]' : count === minCount ? ' [最少]' : '';
    log(`${file.padEnd(20)} ${count.toString().padStart(5)} keys${badge}`, color);
  }
  
  log('');
  
  // 找到基准文件
  if (!localeKeys.has(BASELINE_FILE)) {
    log(`警告: 未找到基准文件 ${BASELINE_FILE}，使用 key 最多的文件作为基准`, 'yellow');
  }
  
  const baselineFile = localeKeys.has(BASELINE_FILE) ? BASELINE_FILE : stats[0].file;
  const baselineKeys = localeKeys.get(baselineFile);
  
  log(`📌 使用 ${baselineFile} 作为基准 (${baselineKeys.size} keys)\n`, 'cyan');
  
  // 比较差异
  log('========================================', 'cyan');
  log('🔍 差异分析', 'bright');
  log('========================================\n', 'cyan');
  
  const differences = new Map();
  
  for (const [file, keys] of localeKeys.entries()) {
    if (file === baselineFile) continue;
    
    const missing = [...baselineKeys].filter(k => !keys.has(k));
    const extra = [...keys].filter(k => !baselineKeys.has(k));
    
    if (missing.length > 0 || extra.length > 0) {
      differences.set(file, { missing, extra });
    }
  }
  
  if (differences.size === 0) {
    log('✅ 所有文件的 key 都与基准文件一致！', 'green');
  } else {
    log(`⚠️  发现 ${differences.size} 个文件存在差异:\n`, 'yellow');
    
    for (const [file, { missing, extra }] of differences.entries()) {
      log(`📄 ${file}`, 'bright');
      
      if (missing.length > 0) {
        log(`   ❌ 缺少 ${missing.length} 个 key (相比 ${baselineFile}):`, 'red');
        missing.slice(0, 10).forEach(key => log(`      - ${key}`, 'red'));
        if (missing.length > 10) {
          log(`      ... 还有 ${missing.length - 10} 个`, 'red');
        }
      }
      
      if (extra.length > 0) {
        log(`   ➕ 多出 ${extra.length} 个 key (相比 ${baselineFile}):`, 'yellow');
        extra.slice(0, 10).forEach(key => log(`      + ${key}`, 'yellow'));
        if (extra.length > 10) {
          log(`      ... 还有 ${extra.length - 10} 个`, 'yellow');
        }
      }
      
      log('');
    }
  }

  // 检测翻译值复用英文
  log('========================================', 'cyan');
  log('🌐 翻译值复用检查', 'bright');
  log('========================================\n', 'cyan');

  const englishReuseIssues = detectEnglishValueReuse(baselineFile, localeValueMaps);
  if (englishReuseIssues.length === 0) {
    log('✅ 未发现“多个非英语语言复用英文值”的问题。', 'green');
  } else {
    log(`⚠️  发现 ${englishReuseIssues.length} 个 key 存在英文值复用（同 key 被多个非英语语言复用）:\n`, 'yellow');
    englishReuseIssues.slice(0, VALUE_REUSE_PREVIEW_LIMIT).forEach(issue => {
      log(`   - ${issue.key}`, 'yellow');
      log(`     value: "${issue.value}"`, 'yellow');
      log(`     locales: ${issue.locales.join(', ')}`, 'yellow');
    });

    if (englishReuseIssues.length > VALUE_REUSE_PREVIEW_LIMIT) {
      log(`\n   ... 还有 ${englishReuseIssues.length - VALUE_REUSE_PREVIEW_LIMIT} 个（详见报告）`, 'yellow');
    }
  }
  log('');

  // 第二阶段：平台分组重复文案检查（仅在英文复用通过后执行）
  log('========================================', 'cyan');
  log('🧭 平台通用 Key 检查', 'bright');
  log('========================================\n', 'cyan');

  let platformCommonIssues = [];
  let platformCommonCheckSkipped = false;

  if (englishReuseIssues.length > 0) {
    platformCommonCheckSkipped = true;
    log('⏭️  已跳过：请先修复“英文值复用”问题，再执行平台通用 Key 检查。', 'yellow');
  } else {
    const baselineData = localeData.get(baselineFile);
    platformCommonIssues = detectPlatformCommonDuplication(baselineData || {});

    if (platformCommonIssues.length === 0) {
      log('✅ 未发现平台分组下可归并到 common.* 的重复文案。', 'green');
    } else {
      log(`⚠️  发现 ${platformCommonIssues.length} 组平台重复文案，建议迁移到 common.*:\n`, 'yellow');
      platformCommonIssues.slice(0, PLATFORM_DUP_PREVIEW_LIMIT).forEach(issue => {
        log(`   - suffix: ${issue.suffix}`, 'yellow');
        log(`     value: "${issue.value}"`, 'yellow');
        log(`     roots: ${issue.roots.join(', ')}`, 'yellow');
        log(`     keys: ${issue.keys.join(', ')}`, 'yellow');
        log(`     suggested: ${issue.suggestedCommonKey}`, 'yellow');
      });

      if (platformCommonIssues.length > PLATFORM_DUP_PREVIEW_LIMIT) {
        log(`\n   ... 还有 ${platformCommonIssues.length - PLATFORM_DUP_PREVIEW_LIMIT} 组（详见报告）`, 'yellow');
      }

      if (!FAIL_ON_PLATFORM_COMMON_DUP) {
        log('\nℹ️  当前为提示模式：如需将该检查设为阻断，请使用 `LOCALE_FAIL_ON_PLATFORM_COMMON_DUP=1 node scripts/check_locales.cjs`。', 'blue');
      }
    }
  }
  log('');
  
  // 生成详细报告
  log('========================================', 'cyan');
  log('📝 生成详细报告', 'bright');
  log('========================================\n', 'cyan');
  
  const reportPath = path.join(__dirname, '../locale-check-report.md');
  generateReport(
    reportPath,
    baselineFile,
    baselineKeys,
    localeKeys,
    differences,
    stats,
    englishReuseIssues,
    platformCommonIssues,
    platformCommonCheckSkipped,
  );
  
  log(`✅ 详细报告已生成: ${reportPath}\n`, 'green');

  const hasBlockingIssues = differences.size > 0
    || englishReuseIssues.length > 0
    || (FAIL_ON_PLATFORM_COMMON_DUP && platformCommonIssues.length > 0);
  if (hasBlockingIssues) {
    log('❌ 检查未通过：请先修复以上问题。', 'red');
    process.exitCode = 1;
  } else {
    log('✅ 检查通过。', 'green');
  }
}

/**
 * 生成 Markdown 报告
 */
function generateReport(
  reportPath,
  baselineFile,
  baselineKeys,
  localeKeys,
  differences,
  stats,
  englishReuseIssues,
  platformCommonIssues,
  platformCommonCheckSkipped,
) {
  let report = '';
  
  report += '# 翻译文件 Key 一致性检查报告\n\n';
  report += `> 生成时间: ${new Date().toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai' })}\n\n`;
  report += `> 基准文件: \`${baselineFile}\` (${baselineKeys.size} keys)\n\n`;
  
  // 统计表格
  report += '## 📊 统计概览\n\n';
  report += '| 文件 | Key 数量 | 相比基准 | 状态 |\n';
  report += '|------|---------|---------|------|\n';
  
  for (const { file, count } of stats) {
    const diff = count - baselineKeys.size;
    const diffStr = diff > 0 ? `+${diff}` : diff < 0 ? `${diff}` : '0';
    const status = diff === 0 ? '✅ 一致' : diff < 0 ? '❌ 缺失' : '➕ 多余';
    const badge = file === baselineFile ? ' **[基准]**' : '';
    report += `| ${file}${badge} | ${count} | ${diffStr} | ${status} |\n`;
  }
  
  report += '\n';
  
  // 差异详情
  if (differences.size > 0) {
    report += '## 🔍 差异详情\n\n';
    
    for (const [file, { missing, extra }] of differences.entries()) {
      report += `### ${file}\n\n`;
      
      if (missing.length > 0) {
        report += `#### ❌ 缺少的 Key (${missing.length} 个)\n\n`;
        report += '<details>\n<summary>点击展开</summary>\n\n';
        report += '```\n';
        missing.forEach(key => report += `${key}\n`);
        report += '```\n\n';
        report += '</details>\n\n';
      }
      
      if (extra.length > 0) {
        report += `#### ➕ 多余的 Key (${extra.length} 个)\n\n`;
        report += '<details>\n<summary>点击展开</summary>\n\n';
        report += '```\n';
        extra.forEach(key => report += `${key}\n`);
        report += '```\n\n';
        report += '</details>\n\n';
      }
    }
  } else {
    report += '## ✅ 完美!\n\n';
    report += '所有翻译文件的 key 都与基准文件保持一致。\n\n';
  }

  // 翻译值复用检查
  report += '## 🌐 翻译值复用检查（非英语语言是否复用英文）\n\n';
  if (englishReuseIssues.length === 0) {
    report += '✅ 未发现“同 key 多个非英语语言复用英文值”的问题。\n\n';
  } else {
    report += `发现 ${englishReuseIssues.length} 个可疑项（同 key 的英文值被多个非英语语言复用）：\n\n`;
    report += '| Key | 英文值（基准） | 复用语言 |\n';
    report += '|-----|----------------|---------|\n';
    for (const issue of englishReuseIssues) {
      const safeValue = issue.value.replace(/\|/g, '\\|');
      report += `| \`${issue.key}\` | ${safeValue} | ${issue.locales.join(', ')} |\n`;
    }
    report += '\n';
  }

  // 平台通用 key 检查
  report += '## 🧭 平台通用 Key 检查（跨平台重复文案归并）\n\n';
  if (platformCommonCheckSkipped) {
    report += '⏭️ 已跳过：需先修复“英文值复用”问题后再检查。\n\n';
  } else if (platformCommonIssues.length === 0) {
    report += '✅ 未发现平台分组下可归并到 `common.*` 的重复文案。\n\n';
  } else {
    report += `发现 ${platformCommonIssues.length} 组可归并项（同 suffix 在多个平台命名空间值一致）：\n\n`;
    report += '| suffix | 文案值（en-US） | 平台 | 当前 key | 建议 common key |\n';
    report += '|--------|----------------|------|----------|------------------|\n';
    for (const issue of platformCommonIssues) {
      const safeValue = issue.value.replace(/\|/g, '\\|');
      const safeKeys = issue.keys.join(', ').replace(/\|/g, '\\|');
      report += `| \`${issue.suffix}\` | ${safeValue} | ${issue.roots.join(', ')} | ${safeKeys} | \`${issue.suggestedCommonKey}\` |\n`;
    }
    report += '\n';
    report += `> 阻断模式：${FAIL_ON_PLATFORM_COMMON_DUP ? '已开启（发现问题将失败）' : '未开启（仅提示）'}\n\n`;
  }
  
  // 所有 key 列表
  report += '## 📋 基准文件所有 Key\n\n';
  report += '<details>\n<summary>点击展开查看所有 key</summary>\n\n';
  report += '```\n';
  [...baselineKeys].sort().forEach(key => report += `${key}\n`);
  report += '```\n\n';
  report += '</details>\n';
  
  fs.writeFileSync(reportPath, report, 'utf8');
}

// 运行
if (require.main === module) {
  main();
}

module.exports = {
  getAllKeys,
  getLeafStringMap,
  getValueByPath,
  isLikelyEnglishValue,
  isAllowedEnglishReuse,
  detectEnglishValueReuse,
  detectPlatformCommonDuplication,
  readJsonFile,
};
