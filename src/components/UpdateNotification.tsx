import { useEffect, useState, useMemo } from 'react';
import { X, Download, Sparkles } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { openUrl } from '@tauri-apps/plugin-opener';
import './UpdateNotification.css';

interface UpdateInfo {
  has_update: boolean;
  latest_version: string;
  current_version: string;
  download_url: string;
  release_notes: string;
  release_notes_zh: string;
}

type UpdateCheckSource = 'auto' | 'manual';
type UpdateCheckStatus = 'has_update' | 'up_to_date' | 'failed';

export interface UpdateCheckResult {
  source: UpdateCheckSource;
  status: UpdateCheckStatus;
  currentVersion?: string;
  latestVersion?: string;
  error?: string;
}

interface UpdateNotificationProps {
  onClose: () => void;
  source?: UpdateCheckSource;
  onResult?: (result: UpdateCheckResult) => void;
}

export const UpdateNotification: React.FC<UpdateNotificationProps> = ({
  onClose,
  source = 'auto',
  onResult,
}) => {
  const { t, i18n } = useTranslation();
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);

  useEffect(() => {
    checkForUpdates();
  }, []);

  const checkForUpdates = async () => {
    try {
      const info = await invoke<UpdateInfo>('check_for_updates');
      if (info.has_update) {
        onResult?.({
          source,
          status: 'has_update',
          currentVersion: info.current_version,
          latestVersion: info.latest_version,
        });
        setUpdateInfo(info);
      } else {
        onResult?.({
          source,
          status: 'up_to_date',
          currentVersion: info.current_version,
          latestVersion: info.latest_version,
        });
        onClose();
      }
    } catch (error) {
      console.error('Failed to check for updates:', error);
      onResult?.({
        source,
        status: 'failed',
        error: String(error),
      });
      onClose();
    }
  };

  const handleDownload = async () => {
    if (updateInfo?.download_url) {
      try {
        await openUrl(updateInfo.download_url);
      } catch {
        // Fallback to window.open if plugin fails
        window.open(updateInfo.download_url, '_blank');
      }
      handleClose();
    }
  };

  const handleClose = () => {
    onClose();
  };

  // 根据语言选择显示中文还是英文更新日志
  const releaseNotes = useMemo(() => {
    if (!updateInfo) return '';
    const isZh = i18n.language.startsWith('zh');
    return isZh && updateInfo.release_notes_zh 
      ? updateInfo.release_notes_zh 
      : updateInfo.release_notes;
  }, [updateInfo, i18n.language]);

  // 简单的 Markdown 渲染
  const formattedNotes = useMemo(() => {
    if (!releaseNotes) return null;
    
    // 解析 Markdown 格式的更新日志
    const lines = releaseNotes.split('\n');
    const elements: React.ReactNode[] = [];
    let key = 0;
    
    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      
      // 标题 (### 或 ##)
      if (trimmed.startsWith('### ')) {
        elements.push(
          <h4 key={key++} className="release-notes-heading">
            {trimmed.slice(4)}
          </h4>
        );
      } else if (trimmed.startsWith('## ')) {
        // 跳过版本号标题
        continue;
      } else if (trimmed.startsWith('- ')) {
        // 列表项
        const content = trimmed.slice(2);
        // 处理加粗 **text**
        const parts = content.split(/\*\*(.*?)\*\*/g);
        elements.push(
          <li key={key++} className="release-notes-item">
            {parts.map((part, i) => 
              i % 2 === 1 ? <strong key={i}>{part}</strong> : part
            )}
          </li>
        );
      }
    }
    
    return elements.length > 0 ? (
      <ul className="release-notes-list">{elements}</ul>
    ) : null;
  }, [releaseNotes]);

  if (!updateInfo) {
    return null;
  }

  return (
    <div className="modal-overlay update-overlay" onClick={handleClose}>
      <div className="modal update-modal" onClick={(event) => event.stopPropagation()}>
        <div className="modal-header">
          <h2 className="update-modal-title">
            <span className="update-icon">
              <Sparkles size={18} />
            </span>
            {t('update_notification.title')}
          </h2>
          <button className="modal-close" onClick={handleClose} aria-label={t('common.cancel')}>
            <X size={18} />
          </button>
        </div>
        <div className="modal-body update-modal-body">
          <div className="update-version">v{updateInfo.latest_version}</div>
          <p className="update-message">
            {t('update_notification.message', { current: updateInfo.current_version })}
          </p>
          
          {formattedNotes && (
            <div className="release-notes">
              <h3 className="release-notes-title">{t('update_notification.whatsNew', "What's New")}</h3>
              <div className="release-notes-content">
                {formattedNotes}
              </div>
            </div>
          )}
        </div>
        <div className="modal-footer">
          <button className="btn btn-secondary" onClick={handleClose}>
            {t('common.cancel')}
          </button>
          <button className="btn btn-primary" onClick={handleDownload}>
            <Download size={16} />
            {t('update_notification.action')}
          </button>
        </div>
      </div>
    </div>
  );
};
