/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useEffect, useState } from 'react';
import {
  Table, Tag, Card, Typography, Space, Button, Input,
  Popconfirm, Switch, App, Tooltip, Row, Col, Drawer, Radio, Pagination
} from 'antd';
import { listPagination, useListPager } from '../../components/ListPagination';
import {
  SyncOutlined,
  PlusOutlined,
  DeleteOutlined,
  CopyOutlined,
  EyeOutlined,
  CloseCircleOutlined,
  StopOutlined,
  CheckCircleOutlined,
  RollbackOutlined,
  ArrowLeftOutlined,
  WalletOutlined,
  ClockCircleOutlined,
  SafetyCertificateOutlined,
  TeamOutlined,
  EditOutlined,
  CheckOutlined,
} from '@ant-design/icons';
import { useTranslation } from 'react-i18next';
import { useLocation, useNavigate } from 'react-router-dom';
import request from '../../utils/request';
import useSettingsStore from '../../store/settings';
import { copyToClipboard as copyToClipboardUtil } from '../../utils/clipboard';
import type { Redemption, RedemptionGroup } from '../../types';
import dayjs from 'dayjs';
import { isRedemptionExpired, isRedemptionNotStarted } from '../../utils/quotaPeriod';

const { Title, Text } = Typography;

function isPlaygroundSpaceType(codeType?: string | null): boolean {
  return (codeType || '').trim() === 'playground_space';
}

function isModelVoucherType(codeType?: string | null): boolean {
  return (codeType || '').trim() === 'model_voucher';
}

function isRedeemedCode(record: Redemption): boolean {
  return (record.used_count ?? 0) > 0 || (record.is_used ?? 0) !== 0;
}

function isRedeemedBalanceCode(record: Redemption): boolean {
  const codeType = (record.code_type || '').trim();
  const balance = codeType === '' || codeType === 'balance';
  return balance && isRedeemedCode(record);
}

function isRedeemedSpaceCode(record: Redemption): boolean {
  return isPlaygroundSpaceType(record.code_type) && isRedeemedCode(record);
}

function isRedeemedVoucherCode(record: Redemption): boolean {
  return isModelVoucherType(record.code_type) && isRedeemedCode(record);
}

function hasOpenUses(record: Redemption): boolean {
  if (record.status === -2) return false;
  const people = record.redeemers || [];
  if (!people.length) return isRedeemedCode(record);
  return people.some((p) => {
    const uses = p.uses || [];
    if (!uses.length) return true;
    return uses.some((u) => (u.refunded ?? 0) === 0);
  });
}

function eligibilitySummary(
  record: {
    condition_match?: number;
    cond_min_balance?: number;
    cond_month_spend?: number;
    cond_total_spend?: number;
    cond_month_recharge?: number;
    cond_total_recharge?: number;
  },
  currencySymbol: string,
  isZh: boolean,
) {
  const items = [
    [record.cond_min_balance, isZh ? '账户余额' : 'Balance'],
    [record.cond_month_spend, isZh ? '当月消费' : 'Spend this month'],
    [record.cond_total_spend, isZh ? '总消费' : 'Total spend'],
    [record.cond_month_recharge, isZh ? '当月充值' : 'Recharge this month'],
    [record.cond_total_recharge, isZh ? '总充值' : 'Total recharge'],
  ].filter((item) => Number(item[0]) > 0);
  if (!items.length) return '';
  const mode = record.condition_match === 1
    ? (isZh ? '满足其中一项' : 'Any one')
    : (isZh ? '全部满足' : 'All required');
  const body = items.map(([amount, label]) => `${label} ≥ ${currencySymbol}${Number(amount)}`).join(isZh ? '；' : '; ');
  return `${mode}：${body}`;
}

function formatRedemptionAmount(amount: number, codeType: string | undefined, currencySymbol: string) {
  if (isPlaygroundSpaceType(codeType)) return `${Math.round(Number(amount) || 0)} MB`;
  const num = Number(amount) || 0;
  const formatted = num.toFixed(6).replace(/(\.\d{2,}?)0+$/, '$1');
  return `${currencySymbol}${formatted}`;
}

type ModelInfo = {
  mid?: string | null;
  name?: string | null;
  model_id?: string | null;
  remark?: string | null;
};

const Redemptions: React.FC = () => {
  const { t, i18n } = useTranslation();
  const { message: msgApi } = App.useApp();
  const isZh = i18n.language === 'zh';
  const navigate = useNavigate();
  const location = useLocation();
  const { settings, updateStoreSettings, fetchSettings } = useSettingsStore();
  const currencySymbol = settings?.currency?.currency_symbol || '$';
  const quotaTz = settings?.site?.default_timezone || 'Asia/Shanghai';
  
  // Group states
  const [groups, setGroups] = useState<RedemptionGroup[]>([]);
  const [modelMap, setModelMap] = useState<Record<string, ModelInfo>>({});
  const [loading, setLoading] = useState(true);
  const { page: currentPage, pageSize, setPage: setCurrentPage, setPageSize } = useListPager();
  const [total, setTotal] = useState(0);

  // Drawer states
  const [drawerVisible, setDrawerVisible] = useState(false);
  const [selectedGroup, setSelectedGroup] = useState<string | null>(null);
  const [selectedGroupInfo, setSelectedGroupInfo] = useState<RedemptionGroup | null>(null);
  const [drawerCodes, setDrawerCodes] = useState<Redemption[]>([]);
  const [drawerLoading, setDrawerLoading] = useState(false);
  const { page: drawerCurrentPage, pageSize: drawerPageSize, setPage: setDrawerCurrentPage, setPageSize: setDrawerPageSize } = useListPager();
  const [drawerTotal, setDrawerTotal] = useState(0);
  const [codeKeyword, setCodeKeyword] = useState('');
  const codeKeywordRef = React.useRef('');

  // Redeeming logs inspector drawer for multi-use codes
  const [activeCodeLogs, setActiveCodeLogs] = useState<Redemption | null>(null);
  const activeCodeLogsRef = React.useRef<Redemption | null>(null);
  const [logSearchText, setLogSearchText] = useState('');
  const [logFilterStatus, setLogFilterStatus] = useState<'all' | 'open' | 'refunded'>('all');
  const [logDrawerPage, setLogDrawerPage] = useState(1);

  const [toggleLoading, setToggleLoading] = useState(false);
  const [enabled, setEnabled] = useState(false);
  const [searchKeyword, setSearchKeyword] = useState('');
  const [codeTypeFilter, setCodeTypeFilter] = useState<string | null>(null);
  const codeTypeFilterRef = React.useRef<string | null>(null);
  codeTypeFilterRef.current = codeTypeFilter;
  const fetchGroups = async (
    page = currentPage,
    size = pageSize,
    search = searchKeyword,
    typeFilter: string | null = codeTypeFilterRef.current,
  ) => {
    setLoading(true);
    try {
      const queryParam = search.trim() ? `&name=${encodeURIComponent(search.trim())}` : '';
      const typeParam = typeFilter ? `&code_type=${encodeURIComponent(typeFilter)}` : '';
      const resp = await (request.get(`/redemptions/groups?page=${page}&page_size=${size}${queryParam}${typeParam}`) as unknown as Promise<{ data: RedemptionGroup[], total: number }>);
      setGroups(resp.data);
      setTotal(resp.total);
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  };

  const handleSearch = (keyword: string) => {
    setSearchKeyword(keyword);
    setCurrentPage(1);
    fetchGroups(1, pageSize, keyword, codeTypeFilterRef.current);
  };

  const fetchDrawerCodes = async (
    name: string,
    page = drawerCurrentPage,
    size = drawerPageSize,
    code = codeKeywordRef.current,
  ) => {
    setDrawerLoading(true);
    try {
      const codeParam = code.trim() ? `&code=${encodeURIComponent(code.trim())}` : '';
      const resp = await (request.get(`/redemptions?name=${encodeURIComponent(name)}&page=${page}&page_size=${size}${codeParam}`) as unknown as Promise<{ data: Redemption[], total: number }>);
      setDrawerCodes(resp.data);
      setDrawerTotal(resp.total);
      return resp.data;
    } catch (e) {
      console.error(e);
      return [];
    } finally {
      setDrawerLoading(false);
    }
  };

  const openDrawer = (group: RedemptionGroup) => {
    codeKeywordRef.current = '';
    setCodeKeyword('');
    setSelectedGroup(group.name);
    setSelectedGroupInfo(group);
    setDrawerVisible(true);
    setDrawerCurrentPage(1);
    fetchDrawerCodes(group.name, 1, drawerPageSize, '');
  };

  const closeDrawer = () => {
    codeKeywordRef.current = '';
    setCodeKeyword('');
    setDrawerVisible(false);
    setSelectedGroup(null);
    setSelectedGroupInfo(null);
    setDrawerCodes([]);
    setActiveCodeLogs(null);
    activeCodeLogsRef.current = null;
  };

  const loadFeatureFlag = async () => {
    try {
      const res = await (request.get('/settings/full') as any);
      const on = !!res?.marketing?.enable_redemption;
      setEnabled(on);
      if (res) {
        updateStoreSettings({
          ...(settings || {}),
          ...res,
        } as any);
      }
    } catch (e) {
      console.error(e);
    }
  };

  const handleToggleRedemption = async (checked: boolean) => {
    setToggleLoading(true);
    try {
      const res = await (request.post('/settings', {
        marketing: { enable_redemption: checked },
      }) as any);
      setEnabled(checked);
      updateStoreSettings(res);
      await fetchSettings(true);
      msgApi.success(
        checked
          ? (isZh ? '已开启兑换功能' : 'Redemption enabled')
          : (isZh ? '已关闭兑换功能' : 'Redemption disabled'),
      );
    } catch (e) {
      console.error(e);
    } finally {
      setToggleLoading(false);
    }
  };

  const copyToClipboard = async (text: string) => {
    const ok = await copyToClipboardUtil(text);
    if (ok) {
      msgApi.success(t('common.copied'));
    } else {
      msgApi.error(isZh ? '复制失败，请手动选择复制' : 'Failed to copy, please select manually');
    }
  };

  const [editingRemarkGroup, setEditingRemarkGroup] = useState<string | null>(null);
  const [editingRemarkDraft, setEditingRemarkDraft] = useState('');
  const [savingRemarkGroup, setSavingRemarkGroup] = useState<string | null>(null);

  const startEditRemark = (groupName: string, currentRemark?: string) => {
    setEditingRemarkGroup(groupName);
    setEditingRemarkDraft(currentRemark || '');
  };

  const cancelEditRemark = () => {
    setEditingRemarkGroup(null);
    setEditingRemarkDraft('');
  };

  const handleSaveGroupRemark = async (groupName: string, rawRemark?: string) => {
    if (savingRemarkGroup === groupName) return;
    const nextRemark = (rawRemark ?? editingRemarkDraft).trim();
    const existingGroup = groups.find((g) => g.name === groupName) || (selectedGroupInfo?.name === groupName ? selectedGroupInfo : undefined);
    if (existingGroup && (existingGroup.remark || '').trim() === nextRemark) {
      setEditingRemarkGroup(null);
      return;
    }
    setSavingRemarkGroup(groupName);
    try {
      const resp = await (request.put('/redemptions/groups/remark', {
        name: groupName,
        remark: nextRemark,
      }) as unknown as Promise<{ success?: boolean; remark?: string }>);
      const saved = resp?.remark ?? nextRemark;
      setGroups((prev) => prev.map((g) => (g.name === groupName ? { ...g, remark: saved } : g)));
      setSelectedGroupInfo((prev) => (prev && prev.name === groupName ? { ...prev, remark: saved } : prev));
      setDrawerCodes((prev) => prev.map((c) => (c.name === groupName ? { ...c, remark: saved } : c)));
      setEditingRemarkGroup(null);
      msgApi.success(isZh ? '备注说明已更新' : 'Remark updated');
    } catch (e) {
      console.error(e);
    } finally {
      setSavingRemarkGroup(null);
    }
  };

  const handleCopyPageCodes = async (withSlogan = false) => {
    if (!drawerCodes.length) {
      msgApi.warning(isZh ? '当前无兑换码可复制' : 'No codes to copy');
      return;
    }
    const slogan = (selectedGroupInfo?.description || drawerCodes[0]?.description || '').trim();
    const codesText = drawerCodes.map((c) => c.code).join('\n');
    const text = withSlogan && slogan ? `${slogan}\n${codesText}` : codesText;
    const ok = await copyToClipboardUtil(text);
    if (ok) {
      msgApi.success(
        withSlogan && slogan
          ? (isZh ? `已复制活动广告语及本页 ${drawerCodes.length} 个兑换码` : `Copied slogan and ${drawerCodes.length} codes`)
          : (isZh ? `已复制本页 ${drawerCodes.length} 个兑换码` : `Copied ${drawerCodes.length} codes`),
      );
    } else {
      msgApi.error(isZh ? '复制失败，请手动选择复制' : 'Failed to copy');
    }
  };

  const handleDeleteGroup = async (name: string) => {
    try {
      await request.delete(`/redemptions/groups?name=${encodeURIComponent(name)}`);
      msgApi.success(t('common.success'));
      fetchGroups(currentPage, pageSize, searchKeyword);
    } catch (e) {
      console.error(e);
    }
  };

  const handleDeleteSingle = async (id: number) => {
    try {
      await request.delete(`/redemptions/${id}`);
      msgApi.success(t('common.success'));
      if (selectedGroup) {
        fetchDrawerCodes(selectedGroup, drawerCurrentPage, drawerPageSize);
        fetchGroups(currentPage, pageSize, searchKeyword); // Update group stats quietly
      }
      if (activeCodeLogsRef.current?.id === id) {
        setActiveCodeLogs(null);
        activeCodeLogsRef.current = null;
      }
    } catch (e) {
      console.error(e);
    }
  };

  const handleUpdateStatus = async (id: number, status: number) => {
    try {
      await request.put(`/redemptions/${id}/status`, { status });
      msgApi.success(t('common.success'));
      if (selectedGroup) {
        const fresh = await fetchDrawerCodes(selectedGroup, drawerCurrentPage, drawerPageSize);
        if (fresh && activeCodeLogsRef.current?.id === id) {
          const next = fresh.find((c) => c.id === id);
          if (next) {
            setActiveCodeLogs(next);
            activeCodeLogsRef.current = next;
          }
        }
        fetchGroups(currentPage, pageSize, searchKeyword);
      }
    } catch (e) {
      console.error(e);
    }
  };

  const handleRefundBalance = async (
    id: number,
    scope?: { user_id?: string; log_id?: number },
    successText?: string,
  ) => {
    try {
      const body = scope?.log_id
        ? { log_id: scope.log_id }
        : scope?.user_id
          ? { user_id: scope.user_id }
          : undefined;
      await request.post(`/redemptions/${id}/refund-balance`, body);
      msgApi.success(successText || (isZh ? '已退还兑换余额' : 'Redeemed balance returned'));
      if (selectedGroup) {
        const fresh = await fetchDrawerCodes(selectedGroup, drawerCurrentPage, drawerPageSize);
        if (fresh && activeCodeLogsRef.current) {
          const next = fresh.find((c) => c.id === activeCodeLogsRef.current?.id);
          if (next) {
            setActiveCodeLogs(next);
            activeCodeLogsRef.current = next;
          }
        }
        fetchGroups(currentPage, pageSize, searchKeyword);
      }
    } catch (e) {
      console.error(e);
    }
  };

  const handleRevokeSpace = async (
    id: number,
    scope?: { user_id?: string; log_id?: number },
    successText?: string,
  ) => {
    try {
      const body = scope?.log_id
        ? { log_id: scope.log_id }
        : scope?.user_id
          ? { user_id: scope.user_id }
          : undefined;
      await request.post(`/redemptions/${id}/revoke-space`, body);
      msgApi.success(successText || (isZh ? '已收回存储空间' : 'Storage revoked'));
      if (selectedGroup) {
        const fresh = await fetchDrawerCodes(selectedGroup, drawerCurrentPage, drawerPageSize);
        if (fresh && activeCodeLogsRef.current) {
          const next = fresh.find((c) => c.id === activeCodeLogsRef.current?.id);
          if (next) {
            setActiveCodeLogs(next);
            activeCodeLogsRef.current = next;
          }
        }
        fetchGroups(currentPage, pageSize, searchKeyword);
      }
    } catch (e) {
      console.error(e);
    }
  };

  const handleRevokeVoucher = async (
    id: number,
    scope?: { user_id?: string; log_id?: number },
    successText?: string,
  ) => {
    try {
      const body = scope?.log_id
        ? { log_id: scope.log_id }
        : scope?.user_id
          ? { user_id: scope.user_id }
          : undefined;
      await request.post(`/redemptions/${id}/revoke-voucher`, body);
      msgApi.success(successText || (isZh ? '已作废模型卡券' : 'Model voucher voided'));
      if (selectedGroup) {
        const fresh = await fetchDrawerCodes(selectedGroup, drawerCurrentPage, drawerPageSize);
        if (fresh && activeCodeLogsRef.current) {
          const next = fresh.find((c) => c.id === activeCodeLogsRef.current?.id);
          if (next) {
            setActiveCodeLogs(next);
            activeCodeLogsRef.current = next;
          }
        }
        fetchGroups(currentPage, pageSize, searchKeyword);
      }
    } catch (e) {
      console.error(e);
    }
  };


  const loadModels = async () => {
    try {
      const res = await (request.get('/models') as unknown as Promise<{ data?: ModelInfo[] }>);
      const map: Record<string, ModelInfo> = {};
      (res?.data || []).forEach((item) => {
        const mid = (item.mid || '').trim();
        const modelId = (item.model_id || '').trim();
        if (mid && !map[mid]) map[mid] = item;
        if (modelId && !map[modelId]) map[modelId] = item;
      });
      setModelMap(map);
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    fetchGroups(1, pageSize);
    loadFeatureFlag();
    loadModels();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const formatExpiryGroup = (expires_at?: string | null) => {
    if (!expires_at) {
      return <Tag color="blue">{isZh ? '长期有效' : 'Permanent'}</Tag>;
    }
    const expired = isRedemptionExpired(expires_at, quotaTz);
    const dateLabel = dayjs(expires_at).tz(quotaTz).format('YYYY-MM-DD HH:mm');
    return (
      <Tag color={expired ? 'error' : 'processing'}>
        {dateLabel}
        {expired ? (isZh ? ' · 已过期' : ' · Expired') : ''}
      </Tag>
    );
  };

  const formatValidity = (starts_at?: string | null, expires_at?: string | null) => {
    const expiry = formatExpiryGroup(expires_at);
    if (!starts_at) return expiry;
    const pending = isRedemptionNotStarted(starts_at, quotaTz);
    const label = dayjs(starts_at).tz(quotaTz).format('YYYY-MM-DD HH:mm');
    return (
      <Space vertical size={2}>
        <Tag color={pending ? 'warning' : 'default'}>
          {label}{isZh ? ' 起' : ' from'}
          {pending ? (isZh ? ' · 未生效' : ' · Pending') : ''}
        </Tag>
        {expiry}
      </Space>
    );
  };

  const limitText = (value?: number) => {
    if (value === undefined || value === -1 || value === 0) return isZh ? '不限制' : 'Unlimited';
    return String(value);
  };

  const parseModelIds = (raw?: string) => {
    if (!raw) return [] as string[];
    try {
      const parsed = JSON.parse(raw);
      return Array.isArray(parsed) ? parsed.map((item) => String(item)).filter(Boolean) : [];
    } catch {
      return [];
    }
  };

  const parseModelDiscountMap = (raw?: string): Record<string, number> => {
    if (!raw) return {};
    try {
      const parsed = JSON.parse(raw);
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
        const out: Record<string, number> = {};
        Object.entries(parsed).forEach(([k, v]) => {
          const num = Number(v);
          if (k && Number.isFinite(num) && num > 0 && num <= 1) {
            out[k] = num;
          }
        });
        return out;
      }
      return {};
    } catch {
      return {};
    }
  };

  const formatVoucherDiscountLabel = (d?: number | null) => {
    const num = Number(d);
    if (!Number.isFinite(num) || num <= 0 || num >= 1) return '';
    const zhe = Number((num * 10).toFixed(2));
    return isZh ? `${zhe}折` : `${Math.round(num * 100)}%`;
  };

  const renderModelTooltipContent = (modelKeys: string[], discMap: Record<string, number> = {}) => (
    <div style={{ maxHeight: 320, overflowY: 'auto', fontSize: 12, lineHeight: 1.5, minWidth: 260 }}>
      <div style={{ fontWeight: 600, marginBottom: 6, borderBottom: '1px solid rgba(255,255,255,0.16)', paddingBottom: 4 }}>
        {isZh ? `限定模型（共 ${modelKeys.length} 个）` : `Allowed Models (${modelKeys.length})`}
      </div>
      {modelKeys.map((rawKey) => {
        const info = modelMap[rawKey];
        const mid = info?.mid || rawKey;
        const name = info?.name || '-';
        const modelId = info?.model_id || rawKey;
        const remark = info?.remark?.trim() || (isZh ? '无备注' : 'No remark');
        const mDisc = formatVoucherDiscountLabel(discMap[rawKey]);
        return (
          <div
            key={rawKey}
            style={{
              padding: '4px 0',
              borderBottom: '1px dashed rgba(255,255,255,0.12)',
            }}
          >
            <div>
              <span style={{ opacity: 0.72 }}>{isZh ? '模型MID：' : 'MID: '}</span>
              <span style={{ fontFamily: 'monospace' }}>{mid}</span>
            </div>
            <div>
              <span style={{ opacity: 0.72 }}>{isZh ? '模型名称：' : 'Name: '}</span>
              <span>{name}</span>
            </div>
            <div>
              <span style={{ opacity: 0.72 }}>{isZh ? '模型ID：' : 'Model ID: '}</span>
              <span style={{ fontFamily: 'monospace' }}>{modelId}</span>
            </div>
            <div>
              <span style={{ opacity: 0.72 }}>{isZh ? '模型备注：' : 'Remark: '}</span>
              <span>{remark}</span>
            </div>
            {mDisc && (
              <div>
                <span style={{ opacity: 0.72 }}>{isZh ? '专属折扣：' : 'Discount: '}</span>
                <span>{mDisc}</span>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );

  const groupColumns = [
    {
      title: isZh ? '活动ID' : 'Activity ID',
      dataIndex: 'activity_id',
      key: 'activity_id',
      width: 190,
      render: (id: string | undefined, record: RedemptionGroup) => {
        const isEditing = editingRemarkGroup === record.name;
        const isSaving = savingRemarkGroup === record.name;
        const remarkText = (record.remark || '').trim();
        return (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
            <div>
              {id ? <Text code>{id}</Text> : <Text type="secondary">-</Text>}
            </div>
            {isEditing ? (
              <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                <Input
                  size="small"
                  value={editingRemarkDraft}
                  maxLength={500}
                  disabled={isSaving}
                  autoFocus
                  placeholder={isZh ? '输入备注，回车保存' : 'Remark, Enter to save'}
                  onChange={(e) => setEditingRemarkDraft(e.target.value)}
                  onPressEnter={() => handleSaveGroupRemark(record.name)}
                  onKeyDown={(e) => {
                    if (e.key === 'Escape') {
                      e.stopPropagation();
                      cancelEditRemark();
                    }
                  }}
                  style={{ width: 140, fontSize: 12 }}
                />
                <Tooltip title={isZh ? '保存备注' : 'Save'}>
                  <Button
                    type="text"
                    size="small"
                    loading={isSaving}
                    icon={<CheckOutlined style={{ fontSize: 12, color: 'var(--ant-color-primary)' }} />}
                    onClick={() => handleSaveGroupRemark(record.name)}
                    style={{ width: 22, height: 22, padding: 0 }}
                  />
                </Tooltip>
                <Tooltip title={isZh ? '取消' : 'Cancel'}>
                  <Button
                    type="text"
                    size="small"
                    disabled={isSaving}
                    icon={<CloseCircleOutlined style={{ fontSize: 12, color: 'var(--ant-color-text-tertiary)' }} />}
                    onClick={cancelEditRemark}
                    style={{ width: 22, height: 22, padding: 0 }}
                  />
                </Tooltip>
              </div>
            ) : (
              <Tooltip
                title={
                  remarkText
                    ? `${isZh ? '管理员备注（点击快速编辑）：' : 'Admin remark (click to edit): '}${remarkText}`
                    : (isZh ? '点击添加管理员备注说明（仅后台可见）' : 'Click to add admin remark (admin only)')
                }
              >
                <div
                  role="button"
                  tabIndex={0}
                  onClick={() => startEditRemark(record.name, record.remark)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      startEditRemark(record.name, record.remark);
                    }
                  }}
                  style={{
                    display: 'inline-flex',
                    alignItems: 'center',
                    gap: 4,
                    maxWidth: 175,
                    cursor: 'pointer',
                    fontSize: 12,
                    lineHeight: 1.35,
                    color: remarkText ? 'var(--ant-color-text-secondary)' : 'var(--ant-color-text-quaternary)',
                  }}
                >
                  <span
                    style={{
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                    }}
                  >
                    {remarkText || (isZh ? '添加备注' : 'Add remark')}
                  </span>
                  <EditOutlined style={{ fontSize: 11, flexShrink: 0, opacity: 0.75 }} />
                </div>
              </Tooltip>
            )}
          </div>
        );
      },
    },
    {
      title: isZh ? '类型' : 'Type',
      dataIndex: 'code_type',
      key: 'code_type',
      width: 132,
      filters: [
        { text: isZh ? '全部' : 'All', value: 'all' },
        { text: isZh ? '余额' : 'Balance', value: 'balance' },
        { text: isZh ? '模型卡券' : 'Model voucher', value: 'model_voucher' },
        { text: isZh ? '创作空间' : 'Creative space', value: 'playground_space' },
      ],
      filterMultiple: false,
      filteredValue: [codeTypeFilter || 'all'],
      filtered: !!codeTypeFilter,
      onFilter: (value: unknown, record: RedemptionGroup) => {
        if (value === 'all') return true;
        const t = record.code_type || 'balance';
        return t === value;
      },
      render: (type?: string) => isPlaygroundSpaceType(type)
        ? <Tag color="cyan">{isZh ? '创作空间' : 'Creative space'}</Tag>
        : type === 'model_voucher'
          ? <Tag color="purple">{isZh ? '模型卡券' : 'Model voucher'}</Tag>
          : <Tag color="blue">{isZh ? '余额' : 'Balance'}</Tag>,
    },
    {
      title: isZh ? '活动名称' : 'Activity Name',
      dataIndex: 'name',
      key: 'name',
      render: (name: string, record: RedemptionGroup) => {
        const slogan = (record.description || '').trim();
        return (
          <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-start', gap: 2 }}>
            <Button
              type="link"
              style={{ padding: 0, height: 'auto', fontWeight: 600 }}
              onClick={() => openDrawer(record)}
            >
              {name}
            </Button>
            {slogan ? (
              <Tooltip title={slogan}>
                <Text
                  type="secondary"
                  style={{
                    fontSize: 12,
                    maxWidth: 260,
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                  }}
                >
                  {slogan}
                </Text>
              </Tooltip>
            ) : null}
          </div>
        );
      },
    },
    {
      title: isZh ? '生成数量' : 'Total Codes',
      key: 'total_count',
      render: (_: unknown, record: RedemptionGroup) => (
        <Space size={8} align="center" style={{ whiteSpace: 'nowrap' }}>
          <Tag color="geekblue">{record.total_count}</Tag>
          <Text type="secondary" style={{ fontSize: 12 }}>
            {isZh ? '已兑换' : 'Redeemed'}: <Text strong>{record.total_used_count ?? 0}</Text>
          </Text>
        </Space>
      ),
    },
    {
      title: isZh ? '总面额' : 'Total Quota',
      dataIndex: 'total_quota',
      key: 'total_quota',
      render: (q: number, record: RedemptionGroup) => {
        const isVoucher = isModelVoucherType(record.code_type);
        const isBalance = !isPlaygroundSpaceType(record.code_type) && !isVoucher;
        const sysQ = Number(q) || 0;
        const giftQ = isBalance ? (Number(record.total_gift_quota) || 0) : 0;
        const totalQ = sysQ + giftQ;
        const worthQ = isVoucher ? (Number(record.total_worth_quota) || 0) : 0;
        const discLabel = isVoucher ? formatVoucherDiscountLabel(record.voucher_discount) : '';
        const discMap = isVoucher ? parseModelDiscountMap(record.model_discounts) : {};
        const overrideCount = isVoucher ? Object.keys(discMap).length : 0;
        const voucherModels = isVoucher ? parseModelIds(record.allowed_models) : [];
        return (
          <div style={{ lineHeight: 1.35 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'wrap' }}>
              <Text strong>{formatRedemptionAmount(totalQ, record.code_type, currencySymbol)}</Text>
              {isVoucher && voucherModels.length > 0 && (
                <Tooltip title={renderModelTooltipContent(voucherModels, discMap)}>
                  <Tag color="cyan" style={{ margin: 0, fontSize: 10, lineHeight: '16px', padding: '0 4px', cursor: 'help' }}>
                    {isZh ? `限 ${voucherModels.length} 模型` : `${voucherModels.length} models`}
                  </Tag>
                </Tooltip>
              )}
            </div>
            {giftQ > 0 && (
              <div style={{ fontSize: 11, color: 'var(--ant-color-text-secondary)' }}>
                {sysQ > 0
                  ? (isZh
                    ? `系统 ${formatRedemptionAmount(sysQ, record.code_type, currencySymbol)} · 赠送金 ${formatRedemptionAmount(giftQ, record.code_type, currencySymbol)}`
                    : `Sys ${formatRedemptionAmount(sysQ, record.code_type, currencySymbol)} · Gift ${formatRedemptionAmount(giftQ, record.code_type, currencySymbol)}`)
                  : (isZh ? '赠送金余额' : 'Gift balance')}
              </div>
            )}
            {isVoucher && (worthQ > 0 || discLabel || overrideCount > 0) && (
              <div style={{ fontSize: 11, color: 'var(--ant-color-text-secondary)' }}>
                {[
                  worthQ > 0 ? `${isZh ? '价值' : 'Worth'} ${formatRedemptionAmount(worthQ, record.code_type, currencySymbol)}` : '',
                  discLabel ? `${isZh ? '卡券' : 'Disc'} ${discLabel}` : '',
                  overrideCount > 0 ? (isZh ? `${overrideCount}模型专属价` : `${overrideCount} custom`) : '',
                ]
                  .filter(Boolean)
                  .join(' · ')}
              </div>
            )}
          </div>
        );
      },
    },
    {
      title: isZh ? '兑换规则' : 'Rule',
      key: 'rule',
      render: (_: unknown, record: RedemptionGroup) => {
        const max = record.max_uses ?? 1;
        const activityLimit = record.per_user_activity_limit ?? -1;
        const maxText = max === -1 || max === 0
          ? (isZh ? '无限' : 'Unlimited')
          : max;
        const activityText = activityLimit === -1 || activityLimit === 0
          ? (isZh ? '不限制' : 'Unlimited')
          : activityLimit;
        return (
          <div style={{ fontSize: '12px', color: 'var(--ant-color-text-secondary)', lineHeight: '1.5' }}>
            <div>{isZh ? '单码可兑:' : 'Per code:'} <Text strong>{maxText}</Text></div>
            <div>{isZh ? '单人活动:' : 'Per user:'} <Text strong>{activityText}</Text></div>
          </div>
        );
      },
    },
    {
      title: isZh ? '有效期' : 'Validity',
      key: 'expires_at',
      render: (_: unknown, record: RedemptionGroup) => formatValidity(record.starts_at, record.expires_at),
    },
    {
      title: t('redemptions.created_at'),
      dataIndex: 'created_at',
      key: 'created_at',
      render: (t: string) => dayjs(t).format('YYYY-MM-DD HH:mm'),
    },
    {
      title: t('common.actions'),
      key: 'actions',
      render: (_: unknown, record: RedemptionGroup) => (
        <Space>
          <Tooltip title={isZh ? '查看明细' : 'View Details'}>
            <Button 
              icon={<EyeOutlined />} 
              size="small" 
              onClick={() => openDrawer(record)} 
            />
          </Tooltip>
          <Popconfirm title={isZh ? `确定要删除活动 [${record.name}] 下的所有兑换码吗？` : `Delete all codes under [${record.name}]?`} onConfirm={() => handleDeleteGroup(record.name)}>
            <Button icon={<DeleteOutlined />} danger size="small" />
          </Popconfirm>
        </Space>
      ),
    },
  ];

  const drawerColumns = [
    {
      title: t('redemptions.code'),
      dataIndex: 'code',
      key: 'code',
      render: (code: string, record: Redemption) => {
        const slogan = (record.description || selectedGroupInfo?.description || '').trim();
        return (
          <Space size={6}>
            <Text
              style={{
                fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
                fontSize: 13,
                fontWeight: 500,
                padding: '2px 8px',
                borderRadius: 4,
                background: 'var(--ant-color-fill-quaternary)',
                border: '1px solid var(--ant-color-border-secondary)',
                letterSpacing: '0.04em',
              }}
            >
              {code}
            </Text>
            <Tooltip title={isZh ? '复制兑换码' : 'Copy code'}>
              <Button
                type="text"
                icon={<CopyOutlined style={{ fontSize: 13 }} />}
                size="small"
                onClick={() => copyToClipboard(code)}
              />
            </Tooltip>
            {slogan ? (
              <Tooltip title={isZh ? '复制含活动广告语' : 'Copy with slogan'}>
                <Button
                  type="text"
                  size="small"
                  style={{ fontSize: 12, padding: '0 4px' }}
                  onClick={() => copyToClipboard(`${slogan}\n${isZh ? '兑换码：' : 'Code: '}${code}`)}
                >
                  {isZh ? '含广告语' : 'w/ slogan'}
                </Button>
              </Tooltip>
            ) : null}
          </Space>
        );
      },
    },
    {
      title: t('redemptions.quota'),
      dataIndex: 'quota',
      key: 'quota',
      render: (q: number, record: Redemption) => {
        const isVoucher = isModelVoucherType(record.code_type);
        const isBalance = !isPlaygroundSpaceType(record.code_type) && !isVoucher;
        const sysQ = Number(q) || 0;
        const giftQ = isBalance ? (Number(record.gift_quota) || 0) : 0;
        const totalQ = sysQ + giftQ;
        const worthQ = isVoucher ? (Number(record.worth_quota) || 0) : 0;
        const discLabel = isVoucher ? formatVoucherDiscountLabel(record.voucher_discount) : '';
        return (
          <div style={{ lineHeight: 1.35 }}>
            <Text strong style={{ fontFamily: 'ui-monospace, monospace', fontSize: 13 }}>
              {formatRedemptionAmount(totalQ, record.code_type, currencySymbol)}
            </Text>
            {giftQ > 0 && (
              <div style={{ fontSize: 11, color: 'var(--ant-color-text-secondary)' }}>
                {sysQ > 0
                  ? (isZh
                    ? `系统 ${formatRedemptionAmount(sysQ, record.code_type, currencySymbol)} · 赠送金 ${formatRedemptionAmount(giftQ, record.code_type, currencySymbol)}`
                    : `Sys ${formatRedemptionAmount(sysQ, record.code_type, currencySymbol)} · Gift ${formatRedemptionAmount(giftQ, record.code_type, currencySymbol)}`)
                  : (isZh ? '赠送金余额' : 'Gift balance')}
              </div>
            )}
            {isVoucher && (worthQ > 0 || discLabel) && (
              <div style={{ fontSize: 11, color: 'var(--ant-color-text-secondary)' }}>
                {[
                  worthQ > 0 ? `${isZh ? '价值' : 'Worth'} ${formatRedemptionAmount(worthQ, record.code_type, currencySymbol)}` : '',
                  discLabel ? `${isZh ? '卡券' : 'Disc'} ${discLabel}` : '',
                ]
                  .filter(Boolean)
                  .join(' · ')}
              </div>
            )}
          </div>
        );
      },
    },
    {
      title: isZh ? '可兑换次数' : 'Allowed Uses',
      key: 'max_uses',
      render: (_: unknown, record: Redemption) => {
        const max = record.max_uses;
        const isUnlimited = max === undefined || max === -1 || max === 0;
        const used = record.used_count ?? (record.is_used ? 1 : 0);
        const maxDisplay = isUnlimited ? (isZh ? '不限' : 'Unlimited') : max;
        return (
          <Tooltip title={isZh ? `可兑上限: ${isUnlimited ? '不限制' : `${max} 次`} · 已兑换: ${used} 次` : `Allowed: ${isUnlimited ? 'Unlimited' : max} · Used: ${used}`}>
            <Text
              strong
              style={{
                fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
                fontSize: 13,
                letterSpacing: '0.02em',
              }}
            >
              {maxDisplay}/{used}
            </Text>
          </Tooltip>
        );
      },
    },
    {
      title: isZh ? '兑换人' : 'Used By',
      key: 'used_by',
      render: (_: unknown, record: Redemption) => {
        const people = record.redeemers || [];
        if (!people.length) return <Text type="secondary">-</Text>;
        const closed = record.status === -2;

        const totalUses = people.reduce((acc, p) => acc + (p.uses?.length || p.times || 1), 0);
        const totalRefunded = people.reduce(
          (acc, p) => acc + (p.uses ? p.uses.filter((u) => (u.refunded ?? 0) !== 0).length : (closed ? 1 : 0)),
          0,
        );

        // 单人且核销笔数 <= 2：单元格展示精简微卡片，附带查看明细入口
        if (people.length === 1 && totalUses <= 2) {
          const person = people[0];
          const uses = person.uses || [];
          return (
            <div
              style={{
                padding: '6px 10px',
                borderRadius: 6,
                background: 'var(--ant-color-fill-quaternary)',
                border: '1px solid var(--ant-color-border-secondary)',
                minWidth: 240,
                maxWidth: 290,
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 6 }}>
                <Space size={6} style={{ minWidth: 0, overflow: 'hidden' }}>
                  <Text strong style={{ fontSize: 13, whiteSpace: 'nowrap' }}>
                    {person.username || '-'}
                  </Text>
                  <Tag
                    bordered={false}
                    style={{
                      fontSize: 11,
                      padding: '0 5px',
                      margin: 0,
                      lineHeight: '18px',
                      background: 'var(--ant-color-fill-secondary)',
                      color: 'var(--ant-color-text-secondary)',
                      fontFamily: 'ui-monospace, monospace',
                    }}
                  >
                    UID: {person.uid || '-'}
                  </Tag>
                </Space>
                <Tooltip title={isZh ? '查看核销明细抽屉' : 'View redemption details drawer'}>
                  <Button
                    size="small"
                    type="text"
                    icon={<EyeOutlined />}
                    style={{ height: 22, width: 22, color: 'var(--ant-color-text-secondary)' }}
                    onClick={() => {
                      setActiveCodeLogs(record);
                      activeCodeLogsRef.current = record;
                      setLogSearchText('');
                      setLogFilterStatus('all');
                      setLogDrawerPage(1);
                    }}
                  />
                </Tooltip>
              </div>

              {uses.map((use, index) => {
                const returned = closed || (use.refunded ?? 0) !== 0;
                const legacy = use.id <= 0;
                const space = isPlaygroundSpaceType(record.code_type);
                const voucher = isModelVoucherType(record.code_type);
                const balance = !space && !voucher;
                const useSys = Number(use.amount) || 0;
                const useGift = balance ? (Number(use.gift_amount) || 0) : 0;
                const useTotal = useSys + useGift;
                return (
                  <div
                    key={use.id > 0 ? use.id : `legacy-${index}`}
                    style={{
                      display: 'flex',
                      gap: 8,
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      marginTop: 4,
                      paddingTop: 3,
                      borderTop: '1px dashed var(--ant-color-border-secondary)',
                    }}
                  >
                    <Space size={6}>
                      <Text type="secondary" style={{ fontSize: 11, fontFamily: 'ui-monospace, monospace' }}>
                        {use.created_at ? dayjs(use.created_at).format('MM-DD HH:mm') : '-'}
                      </Text>
                      <Text style={{ fontSize: 12, fontFamily: 'ui-monospace, monospace', fontWeight: 500 }}>
                        {formatRedemptionAmount(useTotal, record.code_type, currencySymbol)}
                      </Text>
                    </Space>
                    {returned ? (
                      <Tag bordered={false} style={{ margin: 0, fontSize: 10, padding: '0 4px', lineHeight: '18px', background: 'var(--ant-color-fill-secondary)' }}>
                        {space
                          ? (isZh ? '已收回' : 'Revoked')
                          : voucher
                            ? (isZh ? '已作废' : 'Voided')
                            : (isZh ? '已退还' : 'Returned')}
                      </Tag>
                    ) : (space || voucher || balance) ? (
                      <Popconfirm
                        placement="topRight"
                        title={space
                          ? (legacy
                            ? (isZh ? '收回该用户的空间？' : 'Revoke this user?')
                            : (isZh ? '只收回本次空间？' : 'Revoke only this one?'))
                          : voucher
                            ? (isZh ? '只作废本次卡券？' : 'Void only this voucher?')
                            : (legacy
                              ? (isZh ? '退还该用户？' : 'Return this user?')
                              : (isZh ? '只退还本次？' : 'Return only this one?'))}
                        description={(
                          <span style={{ display: 'block', width: 220, whiteSpace: 'normal' }}>
                            {space
                              ? (isZh
                                ? '只收回这一次增加的存储空间。其它次数和其它用户不动，此码也不关闭。已上传的文件不删除。这一次的兑换次数不退回。'
                                : 'Revokes only this storage grant. Other uses stay, and this code stays open. Uploaded files stay. This use is not restored.')
                              : voucher
                                ? (isZh
                                  ? '只作废这一次兑出的模型卡券（清零剩余额度）。其它次数和其它用户不动，此码也不关闭。这一次的兑换次数不退回。'
                                  : 'Voids only the model voucher from this redemption. Other uses and users stay, and this code stays open. This use is not restored.')
                                : legacy
                                  ? (isZh
                                    ? '这是没有逐次记录的历史兑换。退还后关闭此码，不能再兑换。'
                                    : 'This historical redemption has no per-use log. Returning it closes the code.')
                                  : (isZh
                                    ? '只扣回这一次的金额。这个人的其它次数和其它用户不动，此码也不关闭。这一次的兑换次数不退回。次数上限未用尽时，该用户仍可继续兑换。'
                                    : 'Claw back only this amount. Other times and other users stay, and this code stays open. This use is not restored.')}
                          </span>
                        )}
                        okText={space ? (isZh ? '收回' : 'Revoke') : voucher ? (isZh ? '作废卡券' : 'Void voucher') : (isZh ? '退还' : 'Return')}
                        cancelText={isZh ? '取消' : 'Cancel'}
                        okButtonProps={{ danger: true }}
                        onConfirm={() => (space ? handleRevokeSpace : voucher ? handleRevokeVoucher : handleRefundBalance)(
                          record.id,
                          legacy ? { user_id: person.user_id } : { log_id: use.id },
                          space
                            ? (isZh ? '已收回本次存储空间' : 'Storage revoked')
                            : voucher
                              ? (isZh ? '已作废本次模型卡券' : 'Model voucher voided')
                              : legacy
                                ? (isZh ? '已退还该用户的兑换余额' : 'Returned this user\'s balance')
                                : (isZh ? '已退还本次' : 'Returned this redemption'),
                        )}
                      >
                        <Button danger size="small" type="link" style={{ padding: 0, height: 'auto', fontSize: 11 }}>
                          {space
                            ? (isZh ? '收回本次' : 'Revoke this one')
                            : voucher
                              ? (isZh ? '作废卡券' : 'Void voucher')
                              : legacy ? (isZh ? '退还该用户' : 'Return this user') : (isZh ? '退还本次' : 'Return this one')}
                        </Button>
                      </Popconfirm>
                    ) : null}
                  </div>
                );
              })}
            </div>
          );
        }

        // 多位用户（>1 人）或单人多次核销（>2 次）：固定紧凑高度胶囊，提供侧边抽屉呼出
        return (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              padding: '6px 12px',
              borderRadius: 6,
              background: 'var(--ant-color-fill-quaternary)',
              border: '1px solid var(--ant-color-border-secondary)',
              minWidth: 260,
              maxWidth: 320,
              gap: 10,
            }}
          >
            <div style={{ display: 'flex', flexDirection: 'column', gap: 2, minWidth: 0, flex: 1 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'nowrap' }}>
                <TeamOutlined style={{ fontSize: 13, color: 'var(--ant-color-text-secondary)', flexShrink: 0 }} />
                <Text strong style={{ fontSize: 13, whiteSpace: 'nowrap' }}>
                  {isZh ? `共 ${people.length} 位用户` : `${people.length} users`}
                </Text>
                <Tag
                  bordered={false}
                  style={{
                    margin: 0,
                    padding: '0 5px',
                    fontSize: 11,
                    lineHeight: '18px',
                    background: 'var(--ant-color-fill-secondary)',
                    color: 'var(--ant-color-text-secondary)',
                    fontFamily: 'ui-monospace, monospace',
                  }}
                >
                  {isZh ? `${totalUses} 次` : `${totalUses} uses`}
                </Tag>
                {totalRefunded > 0 && (
                  <Tag
                    bordered={false}
                    style={{
                      margin: 0,
                      padding: '0 4px',
                      fontSize: 10,
                      lineHeight: '18px',
                      background: 'var(--ant-color-fill-secondary)',
                      color: 'var(--ant-color-text-tertiary)',
                    }}
                  >
                    {isZh
                      ? `${totalRefunded} ${isPlaygroundSpaceType(record.code_type) ? '收' : isModelVoucherType(record.code_type) ? '废' : '退'}`
                      : `${totalRefunded} rev.`}
                  </Tag>
                )}
              </div>
              <div style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                <Text type="secondary" style={{ fontSize: 11 }}>
                  {people.slice(0, 2).map((p) => p.username || p.uid).join(', ')}
                  {people.length > 2 ? (isZh ? ` 等...` : ` ...`) : ''}
                </Text>
              </div>
            </div>
            <Button
              size="small"
              type="primary"
              ghost
              style={{
                fontSize: 12,
                height: 26,
                padding: '0 8px',
                borderRadius: 4,
                flexShrink: 0,
              }}
              onClick={() => {
                setActiveCodeLogs(record);
                activeCodeLogsRef.current = record;
                setLogSearchText('');
                setLogFilterStatus('all');
                setLogDrawerPage(1);
              }}
            >
              {isZh ? '核销明细' : 'Details'}
            </Button>
          </div>
        );
      },
    },
    {
      title: isZh ? '兑换时间' : 'Used At',
      key: 'used_at',
      render: (_: unknown, record: Redemption) => {
        let latestTime = record.used_at;
        const people = record.redeemers || [];
        let count = 0;
        for (const p of people) {
          for (const u of (p.uses || [])) {
            count++;
            if (u.created_at) {
              if (!latestTime || dayjs(u.created_at).isAfter(dayjs(latestTime))) {
                latestTime = u.created_at;
              }
            }
          }
        }
        if (!latestTime) return <Text type="secondary">-</Text>;
        return (
          <Space vertical size={0}>
            <Text style={{ fontFamily: 'ui-monospace, monospace', fontSize: 12 }}>
              {dayjs(latestTime).format('YYYY-MM-DD HH:mm')}
            </Text>
            {count > 1 && (
              <Text type="secondary" style={{ fontSize: 11 }}>
                {isZh ? '最近核销' : 'Latest'}
              </Text>
            )}
          </Space>
        );
      },
    },
    {
      title: isZh ? '状态' : 'Status',
      key: 'status',
      render: (_: unknown, record: Redemption) => {
        if (record.status === -2) {
          return <Tag bordered={false} style={{ background: 'var(--ant-color-fill-secondary)', color: 'var(--ant-color-text-secondary)', fontSize: 12 }}>{isZh ? '已退还' : 'Returned'}</Tag>;
        }
        if (record.status === -1) {
          return <Tag bordered={false} style={{ background: 'var(--ant-color-fill-secondary)', color: 'var(--ant-color-text-tertiary)', fontSize: 12 }}>{isZh ? '已作废' : 'Voided'}</Tag>;
        }
        if (record.status === 0) {
          return <Tag bordered={false} style={{ background: 'var(--ant-color-fill-secondary)', color: 'var(--ant-color-text-secondary)', fontSize: 12 }}>{isZh ? '已禁用' : 'Disabled'}</Tag>;
        }
        const pending = isRedemptionNotStarted(record.starts_at, quotaTz);
        const expired = isRedemptionExpired(record.expires_at, quotaTz);
        const max = record.max_uses ?? 1;
        const used = record.used_count ?? (record.is_used ? 1 : 0);
        const exhausted = (max > 0 && used >= max) || (!!record.is_used && max === 1);
        if (pending) {
          return (
            <Tag
              bordered={false}
              style={{
                background: 'var(--ant-color-fill-quaternary)',
                color: 'var(--ant-color-text-secondary)',
                border: '1px solid var(--ant-color-border-secondary)',
                fontSize: 12,
              }}
            >
              {isZh ? '未生效' : 'Not yet effective'}
            </Tag>
          );
        }
        if (expired) {
          return (
            <Tag
              bordered={false}
              style={{
                background: 'var(--ant-color-fill-secondary)',
                color: 'var(--ant-color-text-tertiary)',
                fontSize: 12,
              }}
            >
              {isZh ? '已过期' : 'Expired'}
            </Tag>
          );
        }
        if (exhausted) {
          return (
            <Tag
              bordered={false}
              style={{
                background: 'var(--ant-color-fill-secondary)',
                color: 'var(--ant-color-text-secondary)',
                fontSize: 12,
              }}
            >
              {isZh ? '已用完' : 'Exhausted'}
            </Tag>
          );
        }
        return (
          <Tag
            bordered={false}
            style={{
              background: 'var(--ant-color-text)',
              color: 'var(--ant-color-bg-container)',
              fontWeight: 500,
              fontSize: 12,
              padding: '1px 8px',
              borderRadius: 4,
            }}
          >
            {isZh ? '正常' : 'Active'}
          </Tag>
        );
      },
    },
    {
      title: t('common.actions'),
      key: 'actions',
      render: (_: unknown, record: Redemption) => {
        const isClosed = record.status === -2;
        const isVoided = record.status === -1;
        const isDisabled = record.status === 0;
        const isAnyRedeemed = isRedeemedCode(record);
        const isRedeemed = isRedeemedBalanceCode(record);
        const isSpace = isRedeemedSpaceCode(record);
        const isVoucher = isRedeemedVoucherCode(record);
        const openUsesExist = hasOpenUses(record);

        return (
          <Space size={6}>
            {/* 1. 暂停 / 恢复兑换（可逆低风险操作） */}
            {!isVoided && !isClosed && (
              isAnyRedeemed && !isDisabled ? (
                <Popconfirm
                  placement="topRight"
                  title={isZh ? '暂停后续兑换？' : 'Pause further redemption?'}
                  description={(
                    <span style={{ display: 'block', width: 230, whiteSpace: 'normal' }}>
                      {isSpace
                        ? (isZh
                          ? '只停止后续新的兑换，已经发出的存储空间保留。可随时再次启用。'
                          : 'Stops further redemption and keeps storage already granted. Can be re-enabled anytime.')
                        : isVoucher
                          ? (isZh
                            ? '只停止后续新的兑换，已经兑出的模型卡券保留。可随时再次启用。'
                            : 'Stops further redemption and keeps model vouchers already redeemed. Can be re-enabled anytime.')
                          : (isZh
                            ? '只停止后续新的兑换，不扣回已入账余额。可随时再次启用恢复。'
                            : 'Stops further redemption and does not claw back credited balance. Can be re-enabled anytime.')}
                    </span>
                  )}
                  okText={isZh ? '暂停' : 'Pause'}
                  cancelText={isZh ? '取消' : 'Cancel'}
                  onConfirm={() => handleUpdateStatus(record.id, 0)}
                >
                  <Tooltip title={isZh ? '暂停兑换（可随时恢复）' : 'Pause redemption (re-enable anytime)'}>
                    <Button icon={<StopOutlined />} size="small" />
                  </Tooltip>
                </Popconfirm>
              ) : (
                <Tooltip title={isDisabled ? (isZh ? '恢复兑换（重新启用）' : 'Resume redemption') : (isZh ? '暂停兑换（可随时恢复）' : 'Pause redemption (re-enable anytime)')}>
                  <Button
                    icon={isDisabled ? <CheckCircleOutlined /> : <StopOutlined />}
                    size="small"
                    onClick={() => handleUpdateStatus(record.id, isDisabled ? 1 : 0)}
                  />
                </Tooltip>
              )
            )}

            {/* 2. 已核销码：退还所有人余额 / 收回所有人空间 / 作废所有已兑卡券并关闭 */}
            {isRedeemed && !isClosed && openUsesExist && (
              <Popconfirm
                placement="topRight"
                title={isZh ? '退还所有人？' : 'Return every redeemer?'}
                description={(
                  <span style={{ display: 'block', width: 240, whiteSpace: 'normal' }}>
                    {isZh
                      ? '扣回所有尚未退还的记录。同一用户多次兑换合并成一笔扣回。扣回后此码永久关闭，不能再兑换。'
                      : 'Claw back every open redemption. Repeated uses by the same user become one debit. Already returned uses are skipped. This code is then closed.'}
                  </span>
                )}
                okText={isZh ? '退还' : 'Return'}
                cancelText={isZh ? '取消' : 'Cancel'}
                okButtonProps={{ danger: true }}
                onConfirm={() => handleRefundBalance(record.id, undefined, isZh ? '已退还所有人的兑换余额' : 'Returned every redeemer')}
              >
                <Tooltip title={isZh ? '退还所有人余额并关闭' : 'Return everyone and close'}>
                  <Button icon={<RollbackOutlined />} danger size="small" />
                </Tooltip>
              </Popconfirm>
            )}

            {isSpace && !isClosed && openUsesExist && (
              <Popconfirm
                placement="topRight"
                title={isZh ? '收回所有人的空间？' : 'Revoke every grant?'}
                description={(
                  <span style={{ display: 'block', width: 240, whiteSpace: 'normal' }}>
                    {isZh
                      ? '收回这张码尚未收回的存储空间，并永久关闭后续兑换。已上传的文件不删除。已用次数不退回。'
                      : 'Revokes storage still active on this code and closes it. Uploaded files stay. Used counts are not restored.'}
                  </span>
                )}
                okText={isZh ? '收回' : 'Revoke'}
                cancelText={isZh ? '取消' : 'Cancel'}
                okButtonProps={{ danger: true }}
                onConfirm={() => handleRevokeSpace(record.id, undefined, isZh ? '已收回存储空间并关闭兑换码' : 'Storage revoked and code closed')}
              >
                <Tooltip title={isZh ? '收回空间并关闭' : 'Revoke space and close'}>
                  <Button icon={<RollbackOutlined />} danger size="small" />
                </Tooltip>
              </Popconfirm>
            )}

            {isVoucher && !isClosed && openUsesExist && (
              <Popconfirm
                placement="topRight"
                title={isZh ? '作废所有已兑卡券？' : 'Void every redeemed voucher?'}
                description={(
                  <span style={{ display: 'block', width: 240, whiteSpace: 'normal' }}>
                    {isZh
                      ? '作废这张码尚未作废的所有模型卡券（清零剩余额度），并永久关闭后续兑换。已用次数不退回。'
                      : 'Voids all active model vouchers from this code and closes it. Used counts are not restored.'}
                  </span>
                )}
                okText={isZh ? '作废卡券' : 'Void vouchers'}
                cancelText={isZh ? '取消' : 'Cancel'}
                okButtonProps={{ danger: true }}
                onConfirm={() => handleRevokeVoucher(record.id, undefined, isZh ? '已作废所有模型卡券并关闭兑换码' : 'Model vouchers voided and code closed')}
              >
                <Tooltip title={isZh ? '作废所有已兑卡券并关闭' : 'Void redeemed vouchers and close'}>
                  <Button icon={<RollbackOutlined />} danger size="small" />
                </Tooltip>
              </Popconfirm>
            )}

            {/* 3. 作废（永久失效）：未作废且未完全关闭的码均可作废 */}
            {!isVoided && !isClosed && (
              <Popconfirm
                placement="topRight"
                title={isZh ? '确定要作废该兑换码吗？' : 'Are you sure you want to void this code?'}
                description={(
                  <span style={{ display: 'block', width: 230, whiteSpace: 'normal' }}>
                    {isSpace
                      ? (isZh
                        ? '作废后永久关闭后续兑换，不可恢复。已兑换用户的存储空间不受影响，未使用的名额将全部作废。'
                        : 'Permanently stops further redemptions. Granted storage remains unaffected, remaining quotas will be voided.')
                      : isVoucher
                        ? (isZh
                          ? '作废后永久关闭后续兑换，不可恢复。已兑换用户的模型卡券不受影响，未使用的名额将全部作废。'
                          : 'Permanently stops further redemptions. Redeemed model vouchers remain unaffected, remaining quotas will be voided.')
                        : isRedeemed
                          ? (isZh
                            ? '作废后永久关闭后续兑换，不可恢复。已兑换用户的余额不受影响，未使用的名额将全部作废。'
                            : 'Permanently stops further redemptions. Credited balances remain unaffected, remaining quotas will be voided.')
                          : (isZh
                            ? '作废后该码永久失效，不可恢复使用。'
                            : 'Once voided, it cannot be re-enabled.')}
                  </span>
                )}
                okText={isZh ? '作废' : 'Void'}
                cancelText={isZh ? '取消' : 'Cancel'}
                okButtonProps={{ danger: true }}
                onConfirm={() => handleUpdateStatus(record.id, -1)}
              >
                <Tooltip title={isZh ? '作废（永久失效）' : 'Void (permanent)'}>
                  <Button icon={<CloseCircleOutlined />} danger size="small" />
                </Tooltip>
              </Popconfirm>
            )}

            {/* 4. 未核销码：物理删除 */}
            {!isAnyRedeemed && (
              <Popconfirm
                placement="topRight"
                title={isZh ? '确定要删除该兑换码吗？' : t('common.confirm_delete')}
                description={(
                  <span style={{ display: 'block', width: 200, whiteSpace: 'normal' }}>
                    {isZh ? '将从数据库中彻底移除此兑换码。' : 'Permanently remove from database.'}
                  </span>
                )}
                okText={isZh ? '删除' : 'Delete'}
                cancelText={isZh ? '取消' : 'Cancel'}
                okButtonProps={{ danger: true }}
                onConfirm={() => handleDeleteSingle(record.id)}
              >
                <Tooltip title={isZh ? '删除兑换码' : 'Delete'}>
                  <Button icon={<DeleteOutlined />} danger size="small" />
                </Tooltip>
              </Popconfirm>
            )}
          </Space>
        );
      },
    },
  ];

  if (selectedGroup) {
    const isBalanceGroup = !isPlaygroundSpaceType(selectedGroupInfo?.code_type) && !isModelVoucherType(selectedGroupInfo?.code_type);
    const totalSysQuota = Number(selectedGroupInfo?.total_quota) || 0;
    const totalGiftQuota = isBalanceGroup ? (Number(selectedGroupInfo?.total_gift_quota) || 0) : 0;
    const totalCombinedQuota = totalSysQuota + totalGiftQuota;
    const singleQuota = selectedGroupInfo && selectedGroupInfo.total_count > 0
      ? totalCombinedQuota / selectedGroupInfo.total_count
      : 0;
    const singleSysQuota = selectedGroupInfo && selectedGroupInfo.total_count > 0
      ? totalSysQuota / selectedGroupInfo.total_count
      : 0;
    const singleGiftQuota = selectedGroupInfo && selectedGroupInfo.total_count > 0
      ? totalGiftQuota / selectedGroupInfo.total_count
      : 0;

    return (
      <Card
        variant="borderless"
        styles={{ body: { padding: '20px 24px' } }}
      >
        {/* Header navigation & actions */}
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            marginBottom: 20,
            flexWrap: 'wrap',
            gap: 16,
            paddingBottom: 16,
            borderBottom: '1px solid var(--ant-color-border-secondary)',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 12, minWidth: 0 }}>
            <Button
              icon={<ArrowLeftOutlined />}
              onClick={closeDrawer}
              style={{
                borderRadius: 6,
                fontWeight: 500,
              }}
            >
              {isZh ? '返回列表' : 'Back'}
            </Button>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap', minWidth: 0 }}>
              <Title
                level={4}
                style={{
                  margin: 0,
                  fontWeight: 700,
                  letterSpacing: '-0.02em',
                  fontSize: 18,
                }}
              >
                {selectedGroup}
              </Title>
              {selectedGroupInfo && (
                <>
                  <Tag
                    bordered={false}
                    style={{
                      background: 'var(--ant-color-text)',
                      color: 'var(--ant-color-bg-container)',
                      fontWeight: 600,
                      fontSize: 12,
                      padding: '2px 8px',
                      borderRadius: 4,
                      margin: 0,
                    }}
                  >
                    {isPlaygroundSpaceType(selectedGroupInfo.code_type)
                      ? (isZh ? '创作空间' : 'Creative space')
                      : selectedGroupInfo.code_type === 'model_voucher'
                        ? (isZh ? '模型卡券' : 'Model Voucher')
                        : (isZh ? '余额兑换' : 'Balance')}
                  </Tag>
                  {selectedGroupInfo.activity_id ? (
                    <Tag
                      bordered={false}
                      style={{
                        background: 'var(--ant-color-fill-secondary)',
                        color: 'var(--ant-color-text-secondary)',
                        fontFamily: 'ui-monospace, SFMono-Regular, monospace',
                        fontSize: 11,
                        padding: '1px 6px',
                        margin: 0,
                        borderRadius: 4,
                      }}
                    >
                      ID: {selectedGroupInfo.activity_id}
                    </Tag>
                  ) : null}
                </>
              )}
            </div>
          </div>

          <Space size={10} wrap>
            <Input.Search
              placeholder={isZh ? '搜索兑换码' : 'Search code'}
              allowClear
              value={codeKeyword}
              onChange={(e) => {
                const next = e.target.value;
                codeKeywordRef.current = next;
                setCodeKeyword(next);
                if (!next) {
                  setDrawerCurrentPage(1);
                  fetchDrawerCodes(selectedGroup, 1, drawerPageSize, '');
                }
              }}
              onSearch={(value) => {
                codeKeywordRef.current = value;
                setCodeKeyword(value);
                setDrawerCurrentPage(1);
                fetchDrawerCodes(selectedGroup, 1, drawerPageSize, value);
              }}
              style={{ width: 220 }}
            />
            <Button
              icon={<CopyOutlined />}
              onClick={() => handleCopyPageCodes(false)}
            >
              {isZh ? '复制本页码' : 'Copy page codes'}
            </Button>
            {(selectedGroupInfo?.description || drawerCodes[0]?.description)?.trim() && (
              <Button
                icon={<CopyOutlined />}
                onClick={() => handleCopyPageCodes(true)}
              >
                {isZh ? '复制本页含广告语' : 'Copy page w/ slogan'}
              </Button>
            )}
            <Button
              icon={<SyncOutlined />}
              onClick={() => fetchDrawerCodes(selectedGroup, drawerCurrentPage, drawerPageSize)}
            >
              {t('common.refresh')}
            </Button>
          </Space>
        </div>

        {/* 4 Metric Cards */}
        {selectedGroupInfo && (
          <>
            <Row gutter={[16, 16]} style={{ marginBottom: 16 }}>
              {/* Card 1: Value & Total */}
              <Col xs={24} sm={12} lg={6}>
                <div
                  style={{
                    background: 'var(--ant-color-fill-quaternary)',
                    border: '1px solid var(--ant-color-border-secondary)',
                    borderRadius: 10,
                    padding: '16px 18px',
                    height: '100%',
                    display: 'flex',
                    flexDirection: 'column',
                    justifyContent: 'space-between',
                  }}
                >
                  <div>
                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
                      <Text type="secondary" style={{ fontSize: 13, fontWeight: 500 }}>
                        {isPlaygroundSpaceType(selectedGroupInfo.code_type)
                          ? (isZh ? '单码空间' : 'Space')
                          : (isZh ? '单码面额' : 'Face value')}
                      </Text>
                      <WalletOutlined style={{ color: 'var(--ant-color-text-tertiary)', fontSize: 15 }} />
                    </div>
                    <div
                      style={{
                        fontSize: 24,
                        fontWeight: 700,
                        fontFamily: 'ui-monospace, SFMono-Regular, monospace',
                        letterSpacing: '-0.03em',
                        color: 'var(--ant-color-text)',
                      }}
                    >
                      {formatRedemptionAmount(Number(singleQuota), selectedGroupInfo.code_type, currencySymbol)}
                    </div>
                    {isModelVoucherType(selectedGroupInfo.code_type) && (Number(selectedGroupInfo.total_worth_quota) || 0) > 0 && selectedGroupInfo.total_count > 0 && (
                      <div style={{ fontSize: 11, color: 'var(--ant-color-text-secondary)', marginTop: 2 }}>
                        {isZh ? '面额价值 ' : 'Worth '}
                        {formatRedemptionAmount(
                          (Number(selectedGroupInfo.total_worth_quota) || 0) / selectedGroupInfo.total_count,
                          selectedGroupInfo.code_type,
                          currencySymbol,
                        )}
                      </div>
                    )}
                    {isBalanceGroup && singleGiftQuota > 0 && (
                      <div style={{ fontSize: 11, color: 'var(--ant-color-text-secondary)', marginTop: 2 }}>
                        {singleSysQuota > 0
                          ? (isZh
                            ? `系统 ${formatRedemptionAmount(singleSysQuota, selectedGroupInfo.code_type, currencySymbol)} · 赠送金 ${formatRedemptionAmount(singleGiftQuota, selectedGroupInfo.code_type, currencySymbol)}`
                            : `Sys ${formatRedemptionAmount(singleSysQuota, selectedGroupInfo.code_type, currencySymbol)} · Gift ${formatRedemptionAmount(singleGiftQuota, selectedGroupInfo.code_type, currencySymbol)}`)
                          : (isZh ? '赠送金余额' : 'Gift balance')}
                      </div>
                    )}
                  </div>
                  <div
                    style={{
                      fontSize: 12,
                      color: 'var(--ant-color-text-secondary)',
                      marginTop: 10,
                      paddingTop: 8,
                      borderTop: '1px solid var(--ant-color-border-secondary)',
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                    }}
                  >
                    <span>{isPlaygroundSpaceType(selectedGroupInfo.code_type) ? (isZh ? '总空间' : 'Total space') : (isZh ? '总面额' : 'Total')}: <strong style={{ fontFamily: 'ui-monospace, monospace', color: 'var(--ant-color-text)' }}>{formatRedemptionAmount(Number(totalCombinedQuota), selectedGroupInfo.code_type, currencySymbol)}</strong></span>
                    <span>{isZh ? '共' : 'Codes'} <strong style={{ color: 'var(--ant-color-text)' }}>{selectedGroupInfo.total_count}</strong> {isZh ? '个' : ''}</span>
                  </div>
                </div>
              </Col>

              {/* Card 2: Redemptions & Uses */}
              <Col xs={24} sm={12} lg={6}>
                <div
                  style={{
                    background: 'var(--ant-color-fill-quaternary)',
                    border: '1px solid var(--ant-color-border-secondary)',
                    borderRadius: 10,
                    padding: '16px 18px',
                    height: '100%',
                    display: 'flex',
                    flexDirection: 'column',
                    justifyContent: 'space-between',
                  }}
                >
                  <div>
                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
                      <Text type="secondary" style={{ fontSize: 13, fontWeight: 500 }}>
                        {isZh ? '已核销次数' : 'Redeemed uses'}
                      </Text>
                      <CheckCircleOutlined style={{ color: 'var(--ant-color-text-tertiary)', fontSize: 15 }} />
                    </div>
                    <div
                      style={{
                        fontSize: 24,
                        fontWeight: 700,
                        fontFamily: 'ui-monospace, SFMono-Regular, monospace',
                        letterSpacing: '-0.03em',
                        color: 'var(--ant-color-text)',
                      }}
                    >
                      {selectedGroupInfo.total_used_count}
                      <span style={{ fontSize: 13, fontWeight: 400, marginLeft: 4, color: 'var(--ant-color-text-tertiary)' }}>
                        {isZh ? '次' : 'times'}
                      </span>
                    </div>
                  </div>
                  <div
                    style={{
                      fontSize: 12,
                      color: 'var(--ant-color-text-secondary)',
                      marginTop: 10,
                      paddingTop: 8,
                      borderTop: '1px solid var(--ant-color-border-secondary)',
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                    }}
                  >
                    <span>{isZh ? '单码上限' : 'Max/code'}: <strong style={{ color: 'var(--ant-color-text)' }}>{limitText(selectedGroupInfo.max_uses)}</strong></span>
                    <span>{isZh ? '单人单码' : 'Per user'}: <strong style={{ color: 'var(--ant-color-text)' }}>{limitText(selectedGroupInfo.per_user_limit)}</strong></span>
                  </div>
                </div>
              </Col>

              {/* Card 3: Activity Risk Limit */}
              <Col xs={24} sm={12} lg={6}>
                <div
                  style={{
                    background: 'var(--ant-color-fill-quaternary)',
                    border: '1px solid var(--ant-color-border-secondary)',
                    borderRadius: 10,
                    padding: '16px 18px',
                    height: '100%',
                    display: 'flex',
                    flexDirection: 'column',
                    justifyContent: 'space-between',
                  }}
                >
                  <div>
                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
                      <Text type="secondary" style={{ fontSize: 13, fontWeight: 500 }}>
                        {isZh ? '单人活动参与上限' : 'Per user limit'}
                      </Text>
                      <SafetyCertificateOutlined style={{ color: 'var(--ant-color-text-tertiary)', fontSize: 15 }} />
                    </div>
                    <div
                      style={{
                        fontSize: 24,
                        fontWeight: 700,
                        fontFamily: 'ui-monospace, SFMono-Regular, monospace',
                        letterSpacing: '-0.03em',
                        color: 'var(--ant-color-text)',
                      }}
                    >
                      {limitText(selectedGroupInfo.per_user_activity_limit)}
                    </div>
                  </div>
                  <div
                    style={{
                      fontSize: 12,
                      color: 'var(--ant-color-text-secondary)',
                      marginTop: 10,
                      paddingTop: 8,
                      borderTop: '1px solid var(--ant-color-border-secondary)',
                      whiteSpace: 'nowrap',
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                    }}
                  >
                    {selectedGroupInfo.per_user_activity_limit && selectedGroupInfo.per_user_activity_limit > 0
                      ? (isZh ? `全活动单人限兑 ${selectedGroupInfo.per_user_activity_limit} 次` : `Limit ${selectedGroupInfo.per_user_activity_limit} times per user`)
                      : (isZh ? '全活动单人不限制兑换' : 'No activity limit per user')}
                  </div>
                </div>
              </Col>

              {/* Card 4: Validity & Starts */}
              <Col xs={24} sm={12} lg={6}>
                <div
                  style={{
                    background: 'var(--ant-color-fill-quaternary)',
                    border: '1px solid var(--ant-color-border-secondary)',
                    borderRadius: 10,
                    padding: '16px 18px',
                    height: '100%',
                    display: 'flex',
                    flexDirection: 'column',
                    justifyContent: 'space-between',
                  }}
                >
                  <div>
                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
                      <Text type="secondary" style={{ fontSize: 13, fontWeight: 500 }}>
                        {isZh ? '有效期限' : 'Validity'}
                      </Text>
                      <ClockCircleOutlined style={{ color: 'var(--ant-color-text-tertiary)', fontSize: 15 }} />
                    </div>
                    {selectedGroupInfo.expires_at ? (
                      <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                        <span
                          style={{
                            fontSize: 15,
                            fontWeight: 600,
                            fontFamily: 'ui-monospace, monospace',
                            letterSpacing: '-0.01em',
                            color: 'var(--ant-color-text)',
                          }}
                        >
                          {dayjs(selectedGroupInfo.expires_at).tz(quotaTz).format('YYYY-MM-DD HH:mm')}
                        </span>
                        {isRedemptionExpired(selectedGroupInfo.expires_at, quotaTz) ? (
                          <Tag bordered={false} style={{ background: 'var(--ant-color-error-bg)', color: 'var(--ant-color-error-text)', fontSize: 11, margin: 0, padding: '0 5px' }}>
                            {isZh ? '已过期' : 'Expired'}
                          </Tag>
                        ) : (
                          <Tag bordered={false} style={{ background: 'var(--ant-color-fill-secondary)', color: 'var(--ant-color-text-secondary)', fontSize: 11, margin: 0, padding: '0 5px' }}>
                            {isZh ? '有效' : 'Active'}
                          </Tag>
                        )}
                      </div>
                    ) : (
                      <div style={{ fontSize: 18, fontWeight: 700, color: 'var(--ant-color-text)' }}>
                        {isZh ? '长期有效' : 'Permanent'}
                      </div>
                    )}
                  </div>
                  <div
                    style={{
                      fontSize: 12,
                      color: 'var(--ant-color-text-secondary)',
                      marginTop: 10,
                      paddingTop: 8,
                      borderTop: '1px solid var(--ant-color-border-secondary)',
                      display: 'flex',
                      alignItems: 'center',
                      gap: 4,
                      whiteSpace: 'nowrap',
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                    }}
                  >
                    <span>{isZh ? '生效' : 'Starts'}:</span>
                    <span style={{ color: 'var(--ant-color-text)' }}>
                      {selectedGroupInfo.starts_at
                        ? dayjs(selectedGroupInfo.starts_at).tz(quotaTz).format('YYYY-MM-DD HH:mm')
                        : (isZh ? '生成即生效' : 'Immediate')}
                    </span>
                    {selectedGroupInfo.starts_at && isRedemptionNotStarted(selectedGroupInfo.starts_at, quotaTz) ? (
                      <Tag bordered={false} style={{ background: 'var(--ant-color-warning-bg)', color: 'var(--ant-color-warning-text)', fontSize: 10, margin: 0, padding: '0 4px', lineHeight: '16px' }}>
                        {isZh ? '未生效' : 'Pending'}
                      </Tag>
                    ) : null}
                  </div>
                </div>
              </Col>
            </Row>

            {/* Secondary Meta Strip */}
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                flexWrap: 'wrap',
                gap: 12,
                padding: '10px 16px',
                marginBottom: 20,
                borderRadius: 8,
                background: 'var(--ant-color-fill-quaternary)',
                border: '1px solid var(--ant-color-border-secondary)',
                fontSize: 12,
              }}
            >
              <Space size={16} wrap>
                <Text type="secondary" style={{ fontSize: 12 }}>
                  {isZh ? '创建时间' : 'Created'}:{' '}
                  <span style={{ color: 'var(--ant-color-text)', fontFamily: 'ui-monospace, monospace' }}>
                    {dayjs(selectedGroupInfo.created_at).format('YYYY-MM-DD HH:mm:ss')}
                  </span>
                </Text>
                {(selectedGroupInfo.description || drawerCodes[0]?.description)?.trim() && (
                  <>
                    <span style={{ color: 'var(--ant-color-border)' }}>|</span>
                    <Text type="secondary" style={{ fontSize: 12 }}>
                      {isZh ? '活动简介' : 'Activity slogan'}:{' '}
                      <span style={{ color: 'var(--ant-color-text)', fontWeight: 500 }}>
                        {(selectedGroupInfo.description || drawerCodes[0]?.description || '').trim()}
                      </span>
                    </Text>
                  </>
                )}
                <span style={{ color: 'var(--ant-color-border)' }}>|</span>
                <div style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
                  <Text type="secondary" style={{ fontSize: 12 }}>
                    {isZh ? '管理员备注' : 'Admin remark'}:
                  </Text>
                  {editingRemarkGroup === selectedGroupInfo.name ? (
                    <Space size={4}>
                      <Input
                        size="small"
                        value={editingRemarkDraft}
                        maxLength={500}
                        disabled={savingRemarkGroup === selectedGroupInfo.name}
                        autoFocus
                        placeholder={isZh ? '输入备注，回车保存' : 'Remark, Enter to save'}
                        onChange={(e) => setEditingRemarkDraft(e.target.value)}
                        onPressEnter={() => handleSaveGroupRemark(selectedGroupInfo.name)}
                        onKeyDown={(e) => {
                          if (e.key === 'Escape') {
                            e.stopPropagation();
                            cancelEditRemark();
                          }
                        }}
                        style={{ width: 200, fontSize: 12 }}
                      />
                      <Button
                        type="text"
                        size="small"
                        loading={savingRemarkGroup === selectedGroupInfo.name}
                        icon={<CheckOutlined style={{ fontSize: 12, color: 'var(--ant-color-primary)' }} />}
                        onClick={() => handleSaveGroupRemark(selectedGroupInfo.name)}
                        style={{ width: 22, height: 22, padding: 0 }}
                      />
                      <Button
                        type="text"
                        size="small"
                        disabled={savingRemarkGroup === selectedGroupInfo.name}
                        icon={<CloseCircleOutlined style={{ fontSize: 12, color: 'var(--ant-color-text-tertiary)' }} />}
                        onClick={cancelEditRemark}
                        style={{ width: 22, height: 22, padding: 0 }}
                      />
                    </Space>
                  ) : (
                    <span
                      role="button"
                      tabIndex={0}
                      onClick={() => startEditRemark(selectedGroupInfo.name, selectedGroupInfo.remark)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          e.preventDefault();
                          startEditRemark(selectedGroupInfo.name, selectedGroupInfo.remark);
                        }
                      }}
                      style={{
                        display: 'inline-flex',
                        alignItems: 'center',
                        gap: 4,
                        cursor: 'pointer',
                        color: selectedGroupInfo.remark?.trim() ? 'var(--ant-color-text)' : 'var(--ant-color-text-tertiary)',
                        fontWeight: selectedGroupInfo.remark?.trim() ? 500 : 400,
                      }}
                    >
                      <span>{selectedGroupInfo.remark?.trim() || (isZh ? '点击添加备注' : 'Add remark')}</span>
                      <EditOutlined style={{ fontSize: 11, opacity: 0.75 }} />
                    </span>
                  )}
                </div>
                {eligibilitySummary(selectedGroupInfo, currencySymbol, isZh) && (
                  <>
                    <span style={{ color: 'var(--ant-color-border)' }}>|</span>
                    <Text type="secondary" style={{ fontSize: 12 }}>
                      {isZh ? '兑换条件' : 'Eligibility'}:{' '}
                      <span style={{ color: 'var(--ant-color-text)', fontWeight: 500 }}>
                        {eligibilitySummary(selectedGroupInfo, currencySymbol, isZh)}
                      </span>
                    </Text>
                  </>
                )}
                {(selectedGroupInfo.code_type === 'model_voucher' || isPlaygroundSpaceType(selectedGroupInfo.code_type)) && (
                  <>
                    <span style={{ color: 'var(--ant-color-border)' }}>|</span>
                    <Text type="secondary" style={{ fontSize: 12 }}>
                      {isPlaygroundSpaceType(selectedGroupInfo.code_type)
                        ? (isZh ? '创作空间兑换后使用有效期' : 'Storage validity after redeem')
                        : (isZh ? '模型卡券兑换后使用有效期' : 'Voucher validity after redeem')}:{' '}
                      <span style={{ color: 'var(--ant-color-text)', fontWeight: 500 }}>
                        {(selectedGroupInfo.valid_days ?? 0) > 0
                          ? (isZh ? `${selectedGroupInfo.valid_days} 天` : `${selectedGroupInfo.valid_days} days`)
                          : (isZh ? '长期有效' : 'No expiry')}
                      </span>
                    </Text>
                  </>
                )}
                {selectedGroupInfo.code_type === 'model_voucher' && (
                  <>
                    <span style={{ color: 'var(--ant-color-border)' }}>|</span>
                    <div style={{ display: 'inline-flex', alignItems: 'center', gap: 6, flexWrap: 'wrap' }}>
                      <Text type="secondary" style={{ fontSize: 12 }}>{isZh ? '限定模型' : 'Allowed models'}:</Text>
                      {formatVoucherDiscountLabel(selectedGroupInfo.voucher_discount) && (
                        <Tag color="green" style={{ margin: 0, fontSize: 11, padding: '0 6px' }}>
                          {isZh ? `卡券统一 ${formatVoucherDiscountLabel(selectedGroupInfo.voucher_discount)}` : `Coupon ${formatVoucherDiscountLabel(selectedGroupInfo.voucher_discount)}`}
                        </Tag>
                      )}
                      {(() => {
                        const models = Array.from(new Set([
                          ...parseModelIds(selectedGroupInfo.allowed_models),
                          ...drawerCodes.flatMap((code) => parseModelIds(code.allowed_models)),
                        ]));
                        const discMap = {
                          ...parseModelDiscountMap(drawerCodes[0]?.model_discounts),
                          ...parseModelDiscountMap(selectedGroupInfo.model_discounts),
                        };
                        if (!models.length) return <Text type="secondary" style={{ fontSize: 12 }}>-</Text>;
                        return (
                          <Space size={4} wrap>
                            {models.map((m) => {
                              const info = modelMap[m];
                              const mid = info?.mid || m;
                              const name = info?.name || '-';
                              const modelId = info?.model_id || m;
                              const remark = info?.remark?.trim() || (isZh ? '无备注' : 'No remark');
                              const mDisc = formatVoucherDiscountLabel(discMap[m]);
                              return (
                                <Tooltip key={m} title={renderModelTooltipContent([m], discMap)}>
                                  <Tag
                                    bordered={false}
                                    style={{
                                      margin: 0,
                                      fontSize: 11,
                                      padding: '2px 8px',
                                      background: 'var(--ant-color-fill-secondary)',
                                      color: 'var(--ant-color-text)',
                                      borderRadius: 4,
                                      cursor: 'help',
                                    }}
                                  >
                                    <span style={{ fontFamily: 'ui-monospace, monospace' }}>
                                      {isZh ? 'MID' : 'MID'}: {mid}
                                    </span>
                                    {' · '}
                                    <span>
                                      {isZh ? '名称' : 'Name'}: {name}
                                    </span>
                                    {' · '}
                                    <span style={{ fontFamily: 'ui-monospace, monospace' }}>
                                      {isZh ? 'ID' : 'ID'}: {modelId}
                                    </span>
                                    {' · '}
                                    <span>
                                      {isZh ? '备注' : 'Remark'}: {remark}
                                    </span>
                                    {mDisc ? ` · ${mDisc}` : ''}
                                  </Tag>
                                </Tooltip>
                              );
                            })}
                          </Space>
                        );
                      })()}
                    </div>
                  </>
                )}
              </Space>
              <Text type="secondary" style={{ fontSize: 12 }}>
                {isZh ? `已显示 ${drawerCodes.length} / 共 ${drawerTotal} 条` : `Showing ${drawerCodes.length} / Total ${drawerTotal}`}
              </Text>
            </div>
          </>
        )}

        <Table
          dataSource={drawerCodes}
          columns={drawerColumns}
          rowKey="id"
          loading={drawerLoading}
          scroll={{ x: 'max-content' }}
          pagination={listPagination({
            current: drawerCurrentPage,
            pageSize: drawerPageSize,
            total: drawerTotal,
            onChange: (page, size) => {
              setDrawerCurrentPage(page);
              setDrawerPageSize(size);
              fetchDrawerCodes(selectedGroup, page, size);
            },
          })}
        />

        {/* 多次兑换码核销明细侧边抽屉 */}
        <Drawer
          title={
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', paddingRight: 24, flexWrap: 'wrap', gap: 8 }}>
              <Space size={8} wrap>
                <Text strong style={{ fontSize: 16 }}>{isZh ? '核销明细' : 'Redemption Details'}</Text>
                {activeCodeLogs && (
                  <>
                    <Tag
                      bordered={false}
                      style={{
                        margin: 0,
                        fontFamily: 'ui-monospace, monospace',
                        fontSize: 12,
                        background: 'var(--ant-color-fill-secondary)',
                        color: 'var(--ant-color-text)',
                        fontWeight: 500,
                      }}
                    >
                      {activeCodeLogs.code}
                    </Tag>
                    <Tooltip title={isZh ? '复制兑换码' : 'Copy code'}>
                      <Button
                        type="text"
                        size="small"
                        icon={<CopyOutlined />}
                        onClick={() => copyToClipboard(activeCodeLogs.code)}
                        style={{ width: 24, height: 24 }}
                      />
                    </Tooltip>
                  </>
                )}
              </Space>
            </div>
          }
          placement="right"
          width={720}
          open={!!activeCodeLogs}
          onClose={() => {
            setActiveCodeLogs(null);
            activeCodeLogsRef.current = null;
          }}
          styles={{
            body: {
              padding: '16px 20px',
              background: 'var(--ant-color-bg-layout)',
              display: 'flex',
              flexDirection: 'column',
              gap: 16,
            },
          }}
        >
          {(() => {
            if (!activeCodeLogs) return null;
            const allPeople = activeCodeLogs.redeemers || [];
            const isClosed = activeCodeLogs.status === -2;
            const spaceLog = isPlaygroundSpaceType(activeCodeLogs.code_type);
            const voucherLog = isModelVoucherType(activeCodeLogs.code_type);
            const balanceLog = !spaceLog && !voucherLog;

            let totalUses = 0;
            let totalRefundedUses = 0;
            let totalCreditedAmount = 0;

            allPeople.forEach((p) => {
              const uses = p.uses || [];
              if (uses.length > 0) {
                uses.forEach((u) => {
                  totalUses++;
                  if (isClosed || (u.refunded ?? 0) !== 0) {
                    totalRefundedUses++;
                  } else {
                    totalCreditedAmount += (Number(u.amount) || 0) + (balanceLog ? (Number(u.gift_amount) || 0) : 0);
                  }
                });
              } else {
                const t = p.times || 1;
                totalUses += t;
                if (isClosed) {
                  totalRefundedUses += t;
                } else {
                  totalCreditedAmount += (Number(p.amount) || 0) + (balanceLog ? (Number(p.gift_amount) || 0) : 0);
                }
              }
            });

            const activeUses = totalUses - totalRefundedUses;

            // 过滤
            const q = logSearchText.trim().toLowerCase();
            const filteredPeople = allPeople.filter((person) => {
              if (q) {
                const matchName = (person.username || '').toLowerCase().includes(q);
                const matchUid = (person.uid || '').toLowerCase().includes(q);
                const matchRemark = (person.remark || '').toLowerCase().includes(q);
                const matchRefName = (person.referrer_username || '').toLowerCase().includes(q);
                const matchRefUid = (person.referrer_uid || '').toLowerCase().includes(q);
                if (!matchName && !matchUid && !matchRemark && !matchRefName && !matchRefUid) {
                  return false;
                }
              }
              const uses = person.uses || [];
              const allRefunded = isClosed || (uses.length > 0 && uses.every((u) => (u.refunded ?? 0) !== 0));
              if (logFilterStatus === 'open' && allRefunded) return false;
              if (logFilterStatus === 'refunded' && !allRefunded) return false;
              return true;
            });

            const PAGE_SIZE = 10;
            const startIndex = (logDrawerPage - 1) * PAGE_SIZE;
            const pagePeople = filteredPeople.slice(startIndex, startIndex + PAGE_SIZE);

            return (
              <>
                {/* 顶部指标卡片 */}
                <Row gutter={[10, 10]}>
                  <Col span={6}>
                    <div
                      style={{
                        padding: '10px 14px',
                        borderRadius: 8,
                        background: 'var(--ant-color-bg-container)',
                        border: '1px solid var(--ant-color-border-secondary)',
                      }}
                    >
                      <Text type="secondary" style={{ fontSize: 11, display: 'block', marginBottom: 2 }}>
                        {isZh ? '累计核销人数' : 'Total Redeemers'}
                      </Text>
                      <div style={{ display: 'flex', alignItems: 'baseline', gap: 4 }}>
                        <Text strong style={{ fontSize: 18, fontFamily: 'ui-monospace, monospace' }}>
                          {allPeople.length}
                        </Text>
                        <Text type="secondary" style={{ fontSize: 12 }}>{isZh ? '人' : ''}</Text>
                      </div>
                    </div>
                  </Col>
                  <Col span={6}>
                    <div
                      style={{
                        padding: '10px 14px',
                        borderRadius: 8,
                        background: 'var(--ant-color-bg-container)',
                        border: '1px solid var(--ant-color-border-secondary)',
                      }}
                    >
                      <Text type="secondary" style={{ fontSize: 11, display: 'block', marginBottom: 2 }}>
                        {isZh ? '累计核销次数' : 'Total Uses'}
                      </Text>
                      <div style={{ display: 'flex', alignItems: 'baseline', gap: 4 }}>
                        <Text strong style={{ fontSize: 18, fontFamily: 'ui-monospace, monospace' }}>
                          {totalUses}
                        </Text>
                        <Text type="secondary" style={{ fontSize: 12 }}>{isZh ? '次' : ''}</Text>
                      </div>
                    </div>
                  </Col>
                  <Col span={6}>
                    <div
                      style={{
                        padding: '10px 14px',
                        borderRadius: 8,
                        background: 'var(--ant-color-bg-container)',
                        border: '1px solid var(--ant-color-border-secondary)',
                      }}
                    >
                      <Text type="secondary" style={{ fontSize: 11, display: 'block', marginBottom: 2 }}>
                        {spaceLog
                          ? (isZh ? '有效空间' : 'Active space')
                          : voucherLog
                            ? (isZh ? '有效卡券面额' : 'Active Voucher Value')
                            : (isZh ? '有效入账金额' : 'Credited Balance')}
                      </Text>
                      <div style={{ display: 'flex', alignItems: 'baseline', gap: 2 }}>
                        <Text strong style={{ fontSize: 18, fontFamily: 'ui-monospace, monospace' }}>
                          {formatRedemptionAmount(totalCreditedAmount, activeCodeLogs.code_type, currencySymbol)}
                        </Text>
                      </div>
                    </div>
                  </Col>
                  <Col span={6}>
                    <div
                      style={{
                        padding: '10px 14px',
                        borderRadius: 8,
                        background: 'var(--ant-color-bg-container)',
                        border: '1px solid var(--ant-color-border-secondary)',
                      }}
                    >
                      <Text type="secondary" style={{ fontSize: 11, display: 'block', marginBottom: 2 }}>
                        {spaceLog
                          ? (isZh ? '已收回记录' : 'Revoked Uses')
                          : voucherLog
                            ? (isZh ? '已作废卡券' : 'Voided Vouchers')
                            : (isZh ? '已退还记录' : 'Refunded Uses')}
                      </Text>
                      <div style={{ display: 'flex', alignItems: 'baseline', gap: 4 }}>
                        <Text strong style={{ fontSize: 18, fontFamily: 'ui-monospace, monospace', color: totalRefundedUses > 0 ? 'var(--ant-color-text-tertiary)' : undefined }}>
                          {totalRefundedUses}
                        </Text>
                        <Text type="secondary" style={{ fontSize: 12 }}>{isZh ? '笔' : ''}</Text>
                      </div>
                    </div>
                  </Col>
                </Row>

                {/* 搜索与过滤工具栏 */}
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    gap: 12,
                    flexWrap: 'wrap',
                    background: 'var(--ant-color-bg-container)',
                    padding: '10px 14px',
                    borderRadius: 8,
                    border: '1px solid var(--ant-color-border-secondary)',
                  }}
                >
                  <div style={{ display: 'flex', alignItems: 'center', gap: 10, flex: 1, minWidth: 260 }}>
                    <Input
                      placeholder={isZh ? '搜索用户名 / UID / 推荐人...' : 'Search username / UID / referrer...'}
                      allowClear
                      value={logSearchText}
                      onChange={(e) => {
                        setLogSearchText(e.target.value);
                        setLogDrawerPage(1);
                      }}
                      style={{ maxWidth: 280 }}
                      size="middle"
                    />
                    <Radio.Group
                      value={logFilterStatus}
                      onChange={(e) => {
                        setLogFilterStatus(e.target.value);
                        setLogDrawerPage(1);
                      }}
                      size="middle"
                    >
                      <Radio.Button value="all">{isZh ? '全部' : 'All'}</Radio.Button>
                      <Radio.Button value="open">
                        {spaceLog ? (isZh ? '有效空间' : 'Active') : voucherLog ? (isZh ? '有效卡券' : 'Active') : (isZh ? '有效入账' : 'Active')}
                      </Radio.Button>
                      <Radio.Button value="refunded">
                        {spaceLog ? (isZh ? '已收回' : 'Revoked') : voucherLog ? (isZh ? '已作废' : 'Voided') : (isZh ? '已退还' : 'Refunded')}
                      </Radio.Button>
                    </Radio.Group>
                  </div>

                  {/* 批量退还 / 收回 / 作废所有人按钮 */}
                  {activeUses > 0 && !isClosed && balanceLog && (
                    <Popconfirm
                      placement="bottomRight"
                      title={isZh ? '退还所有人？' : 'Return every redeemer?'}
                      description={(
                        <span style={{ display: 'block', width: 240, whiteSpace: 'normal' }}>
                          {isZh
                            ? '扣回所有尚未退还的记录。同一用户多次兑换合并成一笔扣回。扣回后此码永久关闭，不能再兑换。'
                            : 'Claw back every open redemption. Repeated uses by the same user become one debit. This code is then closed.'}
                        </span>
                      )}
                      okText={isZh ? '退还' : 'Return'}
                      cancelText={isZh ? '取消' : 'Cancel'}
                      okButtonProps={{ danger: true }}
                      onConfirm={() => handleRefundBalance(activeCodeLogs.id, undefined, isZh ? '已退还所有人的兑换余额' : 'Returned every redeemer')}
                    >
                      <Button danger size="middle">
                        {isZh ? '退还所有人' : 'Return all'}
                      </Button>
                    </Popconfirm>
                  )}
                  {activeUses > 0 && !isClosed && spaceLog && (
                    <Popconfirm
                      placement="bottomRight"
                      title={isZh ? '收回所有人的空间？' : 'Revoke every grant?'}
                      description={(
                        <span style={{ display: 'block', width: 240, whiteSpace: 'normal' }}>
                          {isZh
                            ? '收回这张码尚未收回的存储空间，并永久关闭后续兑换。已上传的文件不删除。已用次数不退回。'
                            : 'Revokes storage still active on this code and closes it. Uploaded files stay. Used counts are not restored.'}
                        </span>
                      )}
                      okText={isZh ? '收回' : 'Revoke'}
                      cancelText={isZh ? '取消' : 'Cancel'}
                      okButtonProps={{ danger: true }}
                      onConfirm={() => handleRevokeSpace(activeCodeLogs.id, undefined, isZh ? '已收回存储空间并关闭兑换码' : 'Storage revoked and code closed')}
                    >
                      <Button danger size="middle">
                        {isZh ? '收回所有人' : 'Revoke all'}
                      </Button>
                    </Popconfirm>
                  )}
                  {activeUses > 0 && !isClosed && voucherLog && (
                    <Popconfirm
                      placement="bottomRight"
                      title={isZh ? '作废所有已兑卡券？' : 'Void every redeemed voucher?'}
                      description={(
                        <span style={{ display: 'block', width: 240, whiteSpace: 'normal' }}>
                          {isZh
                            ? '作废这张码尚未作废的所有模型卡券（清零剩余额度），并永久关闭后续兑换。已用次数不退回。'
                            : 'Voids all active model vouchers from this code and closes it. Used counts are not restored.'}
                        </span>
                      )}
                      okText={isZh ? '作废卡券' : 'Void vouchers'}
                      cancelText={isZh ? '取消' : 'Cancel'}
                      okButtonProps={{ danger: true }}
                      onConfirm={() => handleRevokeVoucher(activeCodeLogs.id, undefined, isZh ? '已作废所有模型卡券并关闭兑换码' : 'Model vouchers voided and code closed')}
                    >
                      <Button danger size="middle">
                        {isZh ? '作废全部卡券' : 'Void all vouchers'}
                      </Button>
                    </Popconfirm>
                  )}
                </div>

                {/* 列表渲染与分页 */}
                {pagePeople.length === 0 ? (
                  <div
                    style={{
                      padding: '40px 0',
                      textAlign: 'center',
                      background: 'var(--ant-color-bg-container)',
                      borderRadius: 8,
                      border: '1px solid var(--ant-color-border-secondary)',
                    }}
                  >
                    <Text type="secondary">{isZh ? '未检索到相关核销记录' : 'No redemption logs found'}</Text>
                  </div>
                ) : (
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                    {pagePeople.map((person) => {
                      const uses = person.uses || [];
                      const openUses = uses.filter((u) => !isClosed && (u.refunded ?? 0) === 0);
                      const personOpenAmount = openUses.reduce(
                        (acc, u) => acc + (Number(u.amount) || 0) + (balanceLog ? (Number(u.gift_amount) || 0) : 0),
                        0,
                      );

                      return (
                        <div
                          key={person.user_id || `${person.uid}-${person.username}`}
                          style={{
                            padding: '12px 16px',
                            borderRadius: 8,
                            background: 'var(--ant-color-bg-container)',
                            border: '1px solid var(--ant-color-border-secondary)',
                          }}
                        >
                          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', flexWrap: 'wrap', gap: 8, marginBottom: 8 }}>
                            <Space size={8} wrap>
                              <Text strong style={{ fontSize: 14 }}>{person.username || '-'}</Text>
                              <Tag
                                bordered={false}
                                style={{
                                  fontSize: 11,
                                  padding: '0 6px',
                                  margin: 0,
                                  lineHeight: '20px',
                                  background: 'var(--ant-color-fill-secondary)',
                                  color: 'var(--ant-color-text-secondary)',
                                  fontFamily: 'ui-monospace, monospace',
                                }}
                              >
                                UID: {person.uid || '-'}
                              </Tag>
                              {(person.referrer_username || person.referrer_uid) && (
                                <Tag bordered={false} style={{ margin: 0, fontSize: 11, padding: '0 6px', lineHeight: '20px', background: 'var(--ant-color-fill-quaternary)' }}>
                                  {isZh ? '推荐人' : 'Referrer'}: {person.referrer_username || '-'} / {person.referrer_uid || '-'}
                                </Tag>
                              )}
                              {person.remark?.trim() && (
                                <Text type="secondary" style={{ fontSize: 12 }}>({person.remark.trim()})</Text>
                              )}
                            </Space>

                            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                              <Space size={6}>
                                <Text type="secondary" style={{ fontSize: 12 }}>
                                  {isZh ? `核销 ${uses.length || 1} 次` : `${uses.length || 1} uses`}
                                </Text>
                                <Text type="secondary" style={{ fontSize: 12 }}>·</Text>
                                <Text strong style={{ fontSize: 13, fontFamily: 'ui-monospace, monospace' }}>
                                  {isZh ? '有效' : 'Active'}: {formatRedemptionAmount(personOpenAmount, activeCodeLogs.code_type, currencySymbol)}
                                </Text>
                              </Space>

                              {openUses.length > 1 && person.user_id && (balanceLog || spaceLog || voucherLog) && (
                                <Popconfirm
                                  placement="topRight"
                                  title={spaceLog
                                    ? (isZh ? '收回该用户的空间？' : 'Revoke this user?')
                                    : voucherLog
                                      ? (isZh ? '作废该用户的卡券？' : 'Void this user\'s vouchers?')
                                      : (isZh ? '退还该用户？' : 'Return this user?')}
                                  description={(
                                    <span style={{ display: 'block', width: 220, whiteSpace: 'normal' }}>
                                      {spaceLog
                                        ? (isZh
                                          ? '只收回这个人尚未收回的存储空间。其它用户不动，此码也不关闭。已上传的文件不删除。已用次数不退回。'
                                          : 'Revokes only this user\'s open storage. Other users stay, and this code stays open. Uploaded files stay.')
                                        : voucherLog
                                          ? (isZh
                                            ? '只作废这个人尚未作废的模型卡券（清零剩余额度）。其它用户不动，此码也不关闭。已用次数不退回。'
                                            : 'Voids only this user\'s active model vouchers from this code. Other users stay, and this code stays open.')
                                          : (isZh
                                            ? '只扣回这个人尚未退还的次数，多次兑换合并成一笔。其它用户不动，此码也不关闭。已用次数不退回。'
                                            : 'Claw back only this user\'s open redemptions as one debit. Other users stay, and this code stays open.')}
                                    </span>
                                  )}
                                  okText={spaceLog ? (isZh ? '收回' : 'Revoke') : voucherLog ? (isZh ? '作废卡券' : 'Void vouchers') : (isZh ? '退还' : 'Return')}
                                  cancelText={isZh ? '取消' : 'Cancel'}
                                  okButtonProps={{ danger: true }}
                                  onConfirm={() => (spaceLog ? handleRevokeSpace : voucherLog ? handleRevokeVoucher : handleRefundBalance)(
                                    activeCodeLogs.id,
                                    { user_id: person.user_id },
                                    spaceLog
                                      ? (isZh ? '已收回该用户的存储空间' : 'Revoked this user\'s storage')
                                      : voucherLog
                                        ? (isZh ? '已作废该用户的模型卡券' : 'Voided this user\'s model vouchers')
                                        : (isZh ? '已退还该用户的兑换余额' : 'Returned this user\'s balance'),
                                  )}
                                >
                                  <Button danger size="small" type="link" style={{ padding: '0 4px', height: 'auto', fontSize: 12 }}>
                                    {spaceLog
                                      ? (isZh ? '收回该用户' : 'Revoke this user')
                                      : voucherLog
                                        ? (isZh ? '作废该用户卡券' : 'Void this user')
                                        : (isZh ? '退还该用户' : 'Return this user')}
                                  </Button>
                                </Popconfirm>
                              )}
                            </div>
                          </div>

                          {/* 逐笔明细 */}
                          <div style={{ display: 'flex', flexDirection: 'column', gap: 6, paddingTop: 4 }}>
                            {uses.map((use, index) => {
                              const returned = isClosed || (use.refunded ?? 0) !== 0;
                              const legacy = use.id <= 0;
                              const useSysAmt = Number(use.amount) || 0;
                              const useGiftAmt = balanceLog ? (Number(use.gift_amount) || 0) : 0;
                              const useTotalAmt = useSysAmt + useGiftAmt;
                              return (
                                <div
                                  key={use.id > 0 ? use.id : `legacy-${index}`}
                                  style={{
                                    display: 'flex',
                                    alignItems: 'center',
                                    justifyContent: 'space-between',
                                    padding: '6px 12px',
                                    borderRadius: 6,
                                    background: 'var(--ant-color-fill-quaternary)',
                                    border: '1px solid var(--ant-color-border-secondary)',
                                  }}
                                >
                                  <Space size={12}>
                                    <Text type="secondary" style={{ fontSize: 12, fontFamily: 'ui-monospace, monospace' }}>
                                      {use.created_at ? dayjs(use.created_at).format('YYYY-MM-DD HH:mm:ss') : '-'}
                                    </Text>
                                    <Text style={{ fontSize: 13, fontFamily: 'ui-monospace, monospace', fontWeight: 600 }}>
                                      +{formatRedemptionAmount(useTotalAmt, activeCodeLogs.code_type, currencySymbol)}
                                    </Text>
                                    {useGiftAmt > 0 && (
                                      <Text type="secondary" style={{ fontSize: 11 }}>
                                        {useSysAmt > 0
                                          ? (isZh
                                            ? `(系统 ${formatRedemptionAmount(useSysAmt, activeCodeLogs.code_type, currencySymbol)} · 赠送金 ${formatRedemptionAmount(useGiftAmt, activeCodeLogs.code_type, currencySymbol)})`
                                            : `(Sys ${formatRedemptionAmount(useSysAmt, activeCodeLogs.code_type, currencySymbol)} · Gift ${formatRedemptionAmount(useGiftAmt, activeCodeLogs.code_type, currencySymbol)})`)
                                          : (isZh ? '(赠送金)' : '(Gift)')}
                                      </Text>
                                    )}
                                  </Space>

                                  {returned ? (
                                    <Tag bordered={false} style={{ margin: 0, fontSize: 11, background: 'var(--ant-color-fill-secondary)' }}>
                                      {spaceLog
                                        ? (isZh ? '已收回' : 'Revoked')
                                        : voucherLog
                                          ? (isZh ? '已作废' : 'Voided')
                                          : (isZh ? '已退还' : 'Returned')}
                                    </Tag>
                                  ) : (spaceLog || voucherLog || balanceLog) ? (
                                    <Popconfirm
                                      placement="topRight"
                                      title={spaceLog
                                        ? (isZh ? '只收回本次空间？' : 'Revoke only this one?')
                                        : voucherLog
                                          ? (isZh ? '只作废本次卡券？' : 'Void only this voucher?')
                                          : legacy
                                            ? (isZh ? '退还该用户？' : 'Return this user?')
                                            : (isZh ? '只退还本次？' : 'Return only this one?')}
                                      description={(
                                        <span style={{ display: 'block', width: 220, whiteSpace: 'normal' }}>
                                          {spaceLog
                                            ? (isZh
                                              ? '只收回这一次增加的存储空间。其它次数和其它用户不动，此码也不关闭。已上传的文件不删除。这一次的兑换次数不退回。'
                                              : 'Revokes only this storage grant. Other uses stay, and this code stays open. Uploaded files stay.')
                                            : voucherLog
                                              ? (isZh
                                                ? '只作废这一次兑出的模型卡券（清零剩余额度）。其它次数和其它用户不动，此码也不关闭。这一次的兑换次数不退回。'
                                                : 'Voids only the model voucher from this redemption. Other uses and users stay, and this code stays open.')
                                              : legacy
                                                ? (isZh
                                                  ? '这是没有逐次记录的历史兑换。退还后关闭此码，不能再兑换。'
                                                  : 'This historical redemption has no per-use log. Returning it closes the code.')
                                                : (isZh
                                                  ? '只扣回这一次的金额。这个人的其它次数和其它用户不动，此码也不关闭。这一次的兑换次数不退回。'
                                                  : 'Claw back only this amount. Other times and other users stay, and this code stays open.')}
                                        </span>
                                      )}
                                      okText={spaceLog ? (isZh ? '收回' : 'Revoke') : voucherLog ? (isZh ? '作废卡券' : 'Void voucher') : (isZh ? '退还' : 'Return')}
                                      cancelText={isZh ? '取消' : 'Cancel'}
                                      okButtonProps={{ danger: true }}
                                      onConfirm={() => (spaceLog ? handleRevokeSpace : voucherLog ? handleRevokeVoucher : handleRefundBalance)(
                                        activeCodeLogs.id,
                                        legacy ? { user_id: person.user_id } : { log_id: use.id },
                                        spaceLog
                                          ? (isZh ? '已收回本次存储空间' : 'Storage revoked')
                                          : voucherLog
                                            ? (isZh ? '已作废本次模型卡券' : 'Model voucher voided')
                                            : legacy
                                              ? (isZh ? '已退还该用户的兑换余额' : 'Returned this user\'s balance')
                                              : (isZh ? '已退还本次' : 'Returned this redemption'),
                                      )}
                                    >
                                      <Button danger size="small" type="link" style={{ padding: 0, height: 'auto', fontSize: 12 }}>
                                        {spaceLog
                                          ? (isZh ? '收回本次' : 'Revoke this one')
                                          : voucherLog
                                            ? (isZh ? '作废卡券' : 'Void voucher')
                                            : legacy ? (isZh ? '退还该用户' : 'Return this user') : (isZh ? '退还本次' : 'Return this one')}
                                      </Button>
                                    </Popconfirm>
                                  ) : null}
                                </div>
                              );
                            })}
                          </div>
                        </div>
                      );
                    })}

                    {/* 分页器 */}
                    <div style={{ display: 'flex', justifyContent: 'flex-end', paddingTop: 8, paddingBottom: 8 }}>
                      <Pagination
                        size="small"
                        current={logDrawerPage}
                        pageSize={PAGE_SIZE}
                        total={filteredPeople.length}
                        showSizeChanger={false}
                        onChange={(page) => setLogDrawerPage(page)}
                        showTotal={(tot) => (isZh ? `共 ${tot} 位用户` : `Total ${tot} users`)}
                      />
                    </div>
                  </div>
                )}
              </>
            );
          })()}
        </Drawer>
      </Card>
    );
  }

  return (
    <Card variant="borderless">
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 16, flexWrap: 'wrap', gap: 12 }}>
        <Title level={2} style={{ margin: 0 }}>{t('redemptions.title')}</Title>
        <Space wrap>
          <Input.Search
            placeholder={isZh ? '搜索活动名称 / ID / 备注 / 兑换码' : 'Search activity, ID, remark, or code'}
            allowClear
            value={searchKeyword}
            onChange={(e) => {
              setSearchKeyword(e.target.value);
              if (!e.target.value) {
                handleSearch('');
              }
            }}
            onSearch={handleSearch}
            style={{ width: 260 }}
          />
          <Button type="primary" icon={<PlusOutlined />} onClick={() => navigate(`${location.pathname.replace(/\/$/, '')}/new`)}>
            {t('redemptions.add')}
          </Button>
        </Space>
      </div>

      <div
        style={{
          marginBottom: 24,
          padding: '12px 16px',
          borderRadius: 8,
          border: '1px solid var(--ant-color-border)',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 16,
          flexWrap: 'wrap',
        }}
      >
        <div>
          <Text strong>{isZh ? '兑换功能' : 'Redemption Feature'}</Text>
          <div>
            <Text type="secondary" style={{ fontSize: 12 }}>
              {isZh
                ? '开启后，用户可在「钱包与账户」或「卡券中心」使用兑换码兑换余额、模型卡券或创作空间。'
                : 'When enabled, users can redeem codes for balance, model vouchers, or creative space.'}
            </Text>
          </div>
        </div>
        <Switch
          checked={enabled}
          loading={toggleLoading}
          onChange={handleToggleRedemption}
          checkedChildren={isZh ? '开启' : 'On'}
          unCheckedChildren={isZh ? '关闭' : 'Off'}
        />
      </div>

      <Table
        dataSource={groups}
        columns={groupColumns}
        rowKey="name"
        loading={loading}
        scroll={{ x: 'max-content' }}
        onChange={(_pagination, filters, _sorter, extra) => {
          if (extra.action === 'filter') {
            const raw = filters.code_type;
            const selected = raw && raw.length > 0 ? (raw[0] as string) : null;
            const newFilter = (selected === 'all' || !selected) ? null : selected;
            setCodeTypeFilter(newFilter);
            codeTypeFilterRef.current = newFilter;
            setCurrentPage(1);
            fetchGroups(1, pageSize, searchKeyword, newFilter);
          }
        }}
        pagination={listPagination({
          current: currentPage,
          pageSize,
          total,
          onChange: (page, size) => {
            setCurrentPage(page);
            setPageSize(size);
            fetchGroups(page, size, searchKeyword, codeTypeFilterRef.current);
          },
        })}
      />

    </Card>
  );
};

export default Redemptions;
