/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useState, useEffect, useCallback } from 'react';
import { Button, Space, message, Typography, InputNumber } from 'antd';
import request from '../../../utils/request';
import { useThemeStore } from '../../../store/theme';

const { Text } = Typography;

interface PluginLogRetentionCardProps {
  pluginName: string;
  title?: string;
  defaultDays?: number;
  style?: React.CSSProperties;
}

const PluginLogRetentionCard: React.FC<PluginLogRetentionCardProps> = ({
  pluginName,
  title = '日志保留天数',
  defaultDays = 60,
  style,
}) => {
  const { themeMode } = useThemeStore();
  const _isLight = themeMode === 'light';
  const [retentionDays, setRetentionDays] = useState(defaultDays);
  const [savingRetention, setSavingRetention] = useState(false);
  const [loading, setLoading] = useState(false);

  const loadSettings = useCallback(async () => {
    if (!pluginName) return;
    try {
      setLoading(true);
      const res = await (request.get(`/plugins/${pluginName}/log-retention`) as any);
      if (typeof res?.retention_days === 'number') {
        setRetentionDays(res.retention_days);
      }
    } catch {
      // 拦截器已统一提示
    } finally {
      setLoading(false);
    }
  }, [pluginName]);

  useEffect(() => {
    loadSettings();
  }, [loadSettings]);

  const saveRetention = async () => {
    try {
      setSavingRetention(true);
      const res = await (request.put(`/plugins/${pluginName}/log-retention`, {
        retention_days: retentionDays,
      }) as any);
      if (typeof res?.retention_days === 'number') {
        setRetentionDays(res.retention_days);
      }
      message.success('日志保留天数已保存');
    } catch (e) {
      console.error(e);
    } finally {
      setSavingRetention(false);
    }
  };

  return (
    <div
      style={{
        marginBottom: 16,
        padding: '12px 16px',
        borderRadius: 8,
        border: _isLight ? '1px solid #f0f0f0' : '1px solid rgba(255,255,255,0.12)',
        backgroundColor: _isLight ? '#fafafa' : 'rgba(255,255,255,0.02)',
        ...style,
      }}
    >
      <Space wrap align="start">
        <div>
          <Text style={{ display: 'block', marginBottom: 6 }}>{title}</Text>
          <InputNumber
            min={0}
            max={365}
            disabled={loading}
            value={retentionDays}
            onChange={(v) => setRetentionDays(typeof v === 'number' ? v : defaultDays)}
            addonAfter="天"
            style={{ width: 140 }}
          />
          <div style={{ marginTop: 4 }}>
            <Text type="secondary" style={{ fontSize: 12 }}>
              0=关闭自动清理，默认 {defaultDays}；超期记录按站点运营时区每日定时维护（默认 04:30）分批清理
            </Text>
          </div>
        </div>
        <Button
          type="primary"
          onClick={saveRetention}
          loading={savingRetention}
          disabled={loading}
          style={{ marginTop: 22 }}
        >
          保存
        </Button>
      </Space>
    </div>
  );
};

export default PluginLogRetentionCard;
