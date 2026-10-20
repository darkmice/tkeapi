/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useEffect, useState, useMemo } from 'react';
import { Table, Button, Space, Input, Tooltip, message, Popconfirm, Card, Typography, Grid, Checkbox } from 'antd';
import MobileCardList, { MobileCard, CardRow, CardActions } from '../../components/MobileCardList';
import { PlusOutlined, EditOutlined, DeleteOutlined, TrophyOutlined, CheckSquareOutlined, EyeOutlined, EyeInvisibleOutlined, ShoppingOutlined, StopOutlined } from '@ant-design/icons';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import request from '../../utils/request';
import useSettingsStore from '../../store/settings';
import { formatApiDateTime } from '../../utils/timedisplay';
import type { UserLevel } from '../../types';

const { Title, Text } = Typography;
const { useBreakpoint } = Grid;

interface UserLevelsProps {
  embedded?: boolean;
  keyword?: string;
  isBatchEditMode?: boolean;
  onBatchEditModeChange?: (active: boolean) => void;
}

const UserLevels: React.FC<UserLevelsProps> = ({
  embedded = false,
  keyword,
  isBatchEditMode: controlledBatchEdit,
  onBatchEditModeChange,
}) => {
  const { t } = useTranslation();
  const screens = useBreakpoint();
  const navigate = useNavigate();
  const { settings } = useSettingsStore();
  const adminPath = settings?.site?.admin_path || 'admin1688';
  const [levels, setLevels] = useState<UserLevel[]>([]);
  const [loading, setLoading] = useState(true);
  const [searchKeyword, setSearchKeyword] = useState('');
  const [internalBatchEdit, setInternalBatchEdit] = useState(false);
  const [selectedRowKeys, setSelectedRowKeys] = useState<React.Key[]>([]);
  const [batchLoading, setBatchLoading] = useState(false);

  const isBatchEditMode = controlledBatchEdit !== undefined ? controlledBatchEdit : internalBatchEdit;
  const setBatchEditMode = (val: boolean) => {
    if (onBatchEditModeChange) {
      onBatchEditModeChange(val);
    } else {
      setInternalBatchEdit(val);
    }
  };

  useEffect(() => {
    if (!isBatchEditMode) {
      setSelectedRowKeys([]);
    }
  }, [isBatchEditMode]);

  const fetchLevels = async () => {
    setLoading(true);
    try {
      const resp = await (request.get('/user_levels') as unknown as Promise<{ data: UserLevel[] }>);
      setLevels(resp.data);
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchLevels();
  }, []);

  const activeKeyword = keyword !== undefined ? keyword : searchKeyword;

  const filteredLevels = useMemo(() => {
    if (!activeKeyword.trim()) return levels;
    const kw = activeKeyword.trim().toLowerCase();
    return levels.filter((lvl) => {
      const matchName = lvl.name?.toLowerCase().includes(kw);
      const matchId = lvl.id?.toString().includes(kw);
      const matchUlid = `ulid: ${lvl.id.toString().padStart(4, '0')}`.toLowerCase().includes(kw) ||
                        lvl.id.toString().padStart(4, '0').includes(kw);
      const matchKey = lvl.group_key?.toLowerCase().includes(kw);
      const matchDesc = lvl.description?.toLowerCase().includes(kw);
      return matchName || matchId || matchUlid || matchKey || matchDesc;
    });
  }, [levels, activeKeyword]);

  const handleAdd = () => {
    navigate(`/${adminPath}/user-levels/new`);
  };

  const handleEdit = (record: UserLevel) => {
    navigate(`/${adminPath}/user-levels/${record.id}`);
  };

  const handleDelete = async (id: number) => {
    try {
      await request.delete(`/user_levels/${id}`);
      message.success(t('user_levels.success'));
      setSelectedRowKeys((prev) => prev.filter((k) => k !== id));
      fetchLevels();
    } catch (e: any) {
      console.error(e);
    }
  };

  const handleBatchUpdateLogDetails = async (allow: 0 | 1) => {
    if (selectedRowKeys.length === 0) return;
    const selectedKeySet = new Set(selectedRowKeys.map((k) => Number(k)));
    const targetLevels = levels.filter((l) => selectedKeySet.has(l.id));
    if (targetLevels.length === 0) return;
    setBatchLoading(true);
    try {
      await Promise.all(
        targetLevels.map((l) =>
          request.put(`/user_levels/${l.id}`, { allow_view_log_details: allow })
        )
      );
      message.success(`已成功批量${allow === 1 ? '开启' : '关闭'} ${targetLevels.length} 个等级的详细日志`);
      setLevels((prev) =>
        prev.map((l) => (selectedKeySet.has(l.id) ? { ...l, allow_view_log_details: allow } : l))
      );
    } catch (e: any) {
      console.error(e);
      message.error(e?.message || '批量操作失败');
      fetchLevels();
    } finally {
      setBatchLoading(false);
    }
  };

  const handleBatchUpdatePurchasable = async (allow: 0 | 1) => {
    if (selectedRowKeys.length === 0) return;
    const selectedKeySet = new Set(selectedRowKeys.map((k) => Number(k)));
    const targetLevels = levels.filter((l) => selectedKeySet.has(l.id));
    if (targetLevels.length === 0) return;
    setBatchLoading(true);
    try {
      await Promise.all(
        targetLevels.map((l) =>
          request.put(`/user_levels/${l.id}`, { purchasable: allow })
        )
      );
      message.success(`已成功批量${allow === 1 ? '开启' : '关闭'} ${targetLevels.length} 个等级的购买权限`);
      setLevels((prev) =>
        prev.map((l) => (selectedKeySet.has(l.id) ? { ...l, purchasable: allow } : l))
      );
    } catch (e: any) {
      console.error(e);
      message.error(e?.message || '批量操作失败');
      fetchLevels();
    } finally {
      setBatchLoading(false);
    }
  };

  const isProtectedDefaultLevel = (l: UserLevel) => l.group_key === 'default' || l.is_default === 1;

  const handleBatchDelete = async () => {
    if (selectedRowKeys.length === 0) return;
    const selectedKeySet = new Set(selectedRowKeys.map((k) => Number(k)));
    const targetLevels = levels.filter((l) => selectedKeySet.has(l.id) && !isProtectedDefaultLevel(l));
    const skippedDefault = levels.some((l) => selectedKeySet.has(l.id) && isProtectedDefaultLevel(l));

    if (targetLevels.length === 0) {
      message.warning('选中的等级为默认注册等级，默认等级不可删除');
      return;
    }
    setBatchLoading(true);
    try {
      await Promise.all(
        targetLevels.map((l) => request.delete(`/user_levels/${l.id}`))
      );
      if (skippedDefault) {
        message.success(`已删除 ${targetLevels.length} 个会员等级（默认等级已自动保留）`);
      } else {
        message.success(`已成功删除 ${targetLevels.length} 个会员等级`);
      }
      setSelectedRowKeys((prev) => prev.filter((k) => !targetLevels.some((tl) => tl.id === k)));
      fetchLevels();
    } catch (e: any) {
      console.error(e);
      message.error(e?.message || '批量删除失败');
      fetchLevels();
    } finally {
      setBatchLoading(false);
    }
  };

  const handleUpdateSortOrder = async (id: number, sort_order: number) => {
    // 乐观更新：本地状态立刻变更，瞬时生效
    setLevels((prev) => prev.map((m) => (m.id === id ? { ...m, sort_order } : m)));
    try {
      await request.put(`/user_levels/${id}`, { sort_order });
    } catch (e: any) {
      console.error(e);
      message.error(e?.message || '修改排序失败');
      fetchLevels();
    }
  };



  const columns = [
    {
      title: '用户等级ULID',
      dataIndex: 'id',
      key: 'id',
      width: 170,
      sorter: (a: UserLevel, b: UserLevel) => a.id - b.id,
      render: (_: any, record: UserLevel) => (
        <div className="flex flex-col gap-0.5 items-start">
          <span className="inline-flex items-center text-[11px] font-semibold px-1 py-0 rounded border border-border bg-muted/80 text-foreground leading-tight">
            ULID: {record.id.toString().padStart(4, '0')}
          </span>
          <div style={{ maxWidth: 200, display: 'flex' }}>
            <Text
              type="secondary"
              style={{
                fontSize: 11,
                lineHeight: '16px',
                width: '100%',
                color: record.description ? undefined : 'var(--text-tertiary, #8c8c8c)',
                cursor: 'pointer',
              }}
              ellipsis={{ tooltip: record.description || '添加说明备注' }}
              editable={{
                text: record.description || '',
                onChange: async (val) => {
                  if (val === record.description) return;
                  try {
                    await request.put(`/user_levels/${record.id}`, { description: val });
                    setLevels((prev) => prev.map((l) => (l.id === record.id ? { ...l, description: val } : l)));
                    message.success('说明备注已更新');
                  } catch (e: any) {
                    console.error(e);
                    message.error('修改说明备注失败');
                  }
                },
                tooltip: '点击修改说明备注',
                triggerType: ['icon', 'text'],
              }}
            >
              {record.description || <span style={{ opacity: 0.5, fontStyle: 'italic' }}>添加说明备注</span>}
            </Text>
          </div>
        </div>
      ),
    },
    {
      title: t('user_levels.name'),
      dataIndex: 'name',
      key: 'name',
      sorter: (a: UserLevel, b: UserLevel) => a.name.localeCompare(b.name, 'zh'),
      render: (text: string, record: UserLevel) => (
        <div className="flex flex-col gap-0.5 items-start">
          <div style={{ display: 'flex', alignItems: 'center', gap: 5 }}>
            <TrophyOutlined className="text-muted-foreground text-xs" />
            <Text strong style={{ fontSize: 13, lineHeight: '18px' }}>{text}</Text>
          </div>
          {record.is_default === 1 && (
            <span className="inline-flex items-center text-[10px] px-1 py-0 rounded font-medium border border-border bg-foreground text-background leading-tight">
              默认注册
            </span>
          )}
        </div>
      ),
    },
    {
      title: '用户数',
      dataIndex: 'user_count',
      key: 'user_count',
      sorter: (a: UserLevel, b: UserLevel) => (a.user_count || 0) - (b.user_count || 0),
      render: (val: number, record: UserLevel) => (
        <span
          className="inline-flex items-center text-xs px-2 py-0 rounded-full border border-border bg-muted/60 text-foreground font-medium hover:bg-muted transition-colors cursor-pointer select-none leading-5"
          onClick={(e) => {
            e.stopPropagation();
            navigate(`/${adminPath}/users?group=${encodeURIComponent(record.group_key)}`);
          }}
        >
          {val || 0}
        </span>
      ),
    },
    {
      title: t('user_levels.discount'),
      dataIndex: 'discount',
      key: 'discount',
      sorter: (a: UserLevel, b: UserLevel) => (a.discount || 0) - (b.discount || 0),
      render: (val: number, record: UserLevel) => renderDiscountCell(val, record.discount_type),
    },
    {
      title: '等级购买',
      dataIndex: 'purchasable',
      key: 'purchasable',
      sorter: (a: UserLevel, b: UserLevel) => (a.purchasable || 0) - (b.purchasable || 0),
      render: (val: number | undefined) => (
        val === 1 ? (
          <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-border bg-muted/80 text-foreground font-medium leading-tight">已开启</span>
        ) : (
          <span className="inline-flex items-center text-[11px] px-1 py-0.5 text-muted-foreground/60 leading-tight">已关闭</span>
        )
      ),
    },
    {
      title: '等级营销推广',
      key: 'marketing_enabled',
      sorter: (a: UserLevel, b: UserLevel) => (a.marketing_enabled || 0) - (b.marketing_enabled || 0) || (a.commission_ratio || 0) - (b.commission_ratio || 0),
      render: (_: any, record: UserLevel) => {
        const percent = Math.round((record.commission_ratio || 0) * 100);
        const basis = record.commission_basis === 'recharge' ? '充值' : '消费';
        if (record.marketing_enabled !== 1) {
          return (
            <span className="inline-flex items-center text-[11px] px-1 py-0.5 text-muted-foreground/60 leading-tight">已关闭</span>
          );
        }
        return (
          <div className="flex flex-col gap-0.5 items-start">
            <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-border bg-muted/80 text-foreground font-medium leading-tight">已开启</span>
            <span className="inline-flex items-center text-[10px] px-1 py-0 rounded border border-border bg-muted/40 text-muted-foreground leading-tight">{percent}% · {basis}</span>
          </div>
        );
      },
    },
    {
      title: '详细日志',
      dataIndex: 'allow_view_log_details',
      key: 'allow_view_log_details',
      sorter: (a: UserLevel, b: UserLevel) => (a.allow_view_log_details || 0) - (b.allow_view_log_details || 0),
      render: (val: number) => (
        val === 0 ? (
          <span className="inline-flex items-center text-[11px] px-1 py-0.5 text-muted-foreground/60 leading-tight">已关闭</span>
        ) : (
          <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-border bg-muted/80 text-foreground font-medium leading-tight">已开启</span>
        )
      ),
    },
    {
      title: '发票&收据',
      dataIndex: 'invoice_enabled',
      key: 'invoice_enabled',
      sorter: (a: UserLevel, b: UserLevel) => (a.invoice_enabled || 0) - (b.invoice_enabled || 0),
      render: (val: number, record: UserLevel) => (
        val === 1 ? (
          <div className="flex flex-col gap-0.5 items-start">
            <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-border bg-muted/80 text-foreground font-medium leading-tight">已开启</span>
            <span className="inline-flex items-center text-[10px] px-1 py-0 rounded border border-border bg-muted/40 text-muted-foreground leading-tight">
              {record.invoice_mode === 'china' ? '境内模式' : '国际模式'}
            </span>
          </div>
        ) : (
          <span className="inline-flex items-center text-[11px] px-1 py-0.5 text-muted-foreground/60 leading-tight">已关闭</span>
        )
      ),
    },
    {
      title: '页面排序',
      dataIndex: 'sort_order',
      key: 'sort_order',
      width: 80,
      align: 'center' as const,
      sorter: (a: UserLevel, b: UserLevel) => (a.sort_order || 0) - (b.sort_order || 0),
      render: (_: any, record: UserLevel) => {
        const order = record.sort_order ?? 0;
        return (
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <Text
              editable={{
                tooltip: false,
                text: String(order),
                onChange: (newVal) => {
                  const num = parseInt(newVal.trim(), 10);
                  handleUpdateSortOrder(record.id, isNaN(num) ? 0 : num);
                },
              }}
              style={{
                fontSize: 12,
                fontWeight: 600,
                padding: '2px 8px',
                borderRadius: 6,
                background: order > 0 ? 'rgba(59, 130, 246, 0.08)' : 'rgba(128, 128, 128, 0.06)',
                color: order > 0 ? '#2563eb' : 'var(--text-secondary, #8c8c8c)',
                border: order > 0 ? '1px solid rgba(59, 130, 246, 0.2)' : '1px solid rgba(128, 128, 128, 0.15)',
                cursor: 'pointer',
                transition: 'all 0.2s',
              }}
            >
              {order}
            </Text>
          </div>
        );
      },
    },
    {
      title: t('user_levels.created_at'),
      dataIndex: 'created_at',
      key: 'created_at',
      sorter: (a: UserLevel, b: UserLevel) => {
        const timeA = a.created_at ? new Date(a.created_at).getTime() : 0;
        const timeB = b.created_at ? new Date(b.created_at).getTime() : 0;
        return timeA - timeB;
      },
      render: (text: string) => <span className="text-xs text-muted-foreground">{formatApiDateTime(text, 'YYYY-MM-DD')}</span>,
    },
    {
      title: t('common.actions'),
      key: 'actions',
      render: (_: any, record: UserLevel) => (
        <Space size={2}>
          <Button type="text" size="small" icon={<EditOutlined className="text-muted-foreground" />} onClick={() => handleEdit(record)} />
          <Popconfirm 
            title={t('user_levels.delete_confirm')} 
            onConfirm={() => handleDelete(record.id)}
            disabled={isProtectedDefaultLevel(record)}
          >
            <Button type="text" size="small" icon={<DeleteOutlined />} danger disabled={isProtectedDefaultLevel(record)} />
          </Popconfirm>
        </Space>
      ),
    },
  ];

  const renderDiscountCell = (val: number, discountType?: number) => {
    const off = Math.round((1 - val) * 100);
    const up = Math.round((val - 1) * 100);
    const v = discountType ?? 0;
    const mask = v >= 16 ? (v & 15) : (v === 1 ? 6 : v === 2 ? 3 : 15);
    const hasLevelDisc = (mask & 1) !== 0;
    const labels = [
      hasLevelDisc ? '用户等级折扣' : '',
      (mask & 2) ? '用户模型折扣' : '',
      (mask & 4) ? '模型全站折扣' : '',
      (mask & 8) ? '上游渠道倍率' : '',
    ].filter(Boolean);
    return (
      <div className="flex flex-col gap-0.5 items-start">
        {hasLevelDisc ? (
          <span className="text-xs text-foreground leading-tight">{val.toFixed(2)}x</span>
        ) : (
          <span className="text-xs text-muted-foreground leading-tight" title="未勾选用户等级折扣，此等级倍率未启用">
            <span className="line-through">{val.toFixed(2)}x</span>
            <span className="ml-1 text-[10px]">(未启用)</span>
          </span>
        )}
        <div className="flex flex-wrap gap-1 items-center">
          {(labels.length > 0 ? labels : ['不享受折扣 (1.00x 原价)']).map(lbl => (
            <span key={lbl} className="inline-flex items-center text-[10px] px-1 py-0 rounded border border-border bg-muted/40 text-muted-foreground leading-tight">{lbl}</span>
          ))}
          {hasLevelDisc && off > 0 && <span className="inline-flex items-center text-[10px] px-1 py-0 rounded border border-border bg-muted text-foreground font-medium leading-tight">-{off}% (优惠)</span>}
          {hasLevelDisc && up > 0 && <span className="inline-flex items-center text-[10px] px-1 py-0 rounded border border-border bg-muted text-foreground font-medium leading-tight">+{up}% (涨价)</span>}
        </div>
      </div>
    );
  };

  const body = (
    <>
      {!embedded && (
      <div style={{ display: 'flex', flexDirection: screens.xs ? 'column' : 'row', justifyContent: 'space-between', marginBottom: 12, gap: 12 }}>
        <Title level={screens.xs ? 4 : 2} style={{ margin: 0 }}>{t('user_levels.title')}</Title>
        <Space wrap>
          <Input.Search
            placeholder="搜索等级名称 / 等级 ID"
            allowClear
            value={searchKeyword}
            onChange={(e) => setSearchKeyword(e.target.value)}
            onSearch={(val) => setSearchKeyword(val)}
            style={{ width: screens.xs ? '100%' : 220 }}
          />
          <Button
            icon={<CheckSquareOutlined />}
            type={isBatchEditMode ? 'primary' : 'default'}
            danger={isBatchEditMode}
            onClick={() => setBatchEditMode(!isBatchEditMode)}
          >
            {isBatchEditMode ? '退出选择' : '选择编辑'}
          </Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={handleAdd}>{t('user_levels.add_level')}</Button>
        </Space>
      </div>
      )}

      {/* 批量操作工具条 */}
      {isBatchEditMode && (
        <div className="flex items-center justify-between flex-wrap gap-2 px-3 py-1.5 mb-2 rounded-lg border border-border bg-muted/40 transition-all">
          <Space wrap size={8} style={{ alignItems: 'center' }}>
            <Button
              size="small"
              onClick={() => {
                const selectableKeys = filteredLevels.map((l) => l.id);
                setSelectedRowKeys(selectableKeys);
              }}
            >
              全选
            </Button>
            <Button
              size="small"
              disabled={selectedRowKeys.length === 0}
              onClick={() => setSelectedRowKeys([])}
            >
              取消选择
            </Button>
            <span className="text-xs text-foreground font-medium ml-1">
              已选择 <strong className="font-semibold text-foreground">{selectedRowKeys.length}</strong> 项
            </span>
          </Space>

          <Space wrap size={8}>
            <Button
              size="small"
              icon={<ShoppingOutlined />}
              onClick={() => handleBatchUpdatePurchasable(1)}
              disabled={selectedRowKeys.length === 0 || batchLoading}
              loading={batchLoading}
            >
              开启等级购买
            </Button>
            <Button
              size="small"
              icon={<StopOutlined />}
              onClick={() => handleBatchUpdatePurchasable(0)}
              disabled={selectedRowKeys.length === 0 || batchLoading}
              loading={batchLoading}
            >
              关闭等级购买
            </Button>
            <Button
              size="small"
              icon={<EyeOutlined />}
              onClick={() => handleBatchUpdateLogDetails(1)}
              disabled={selectedRowKeys.length === 0 || batchLoading}
              loading={batchLoading}
            >
              开启详细日志
            </Button>
            <Button
              size="small"
              icon={<EyeInvisibleOutlined />}
              onClick={() => handleBatchUpdateLogDetails(0)}
              disabled={selectedRowKeys.length === 0 || batchLoading}
              loading={batchLoading}
            >
              关闭详细日志
            </Button>
            <Popconfirm
              title="确定要批量删除选中的会员等级吗？"
              description={`将删除选中的 ${selectedRowKeys.length} 个等级（默认等级受保护不可删除）。此操作不可撤销。`}
              onConfirm={handleBatchDelete}
              okText="确认删除"
              cancelText="取消"
              okButtonProps={{ danger: true, loading: batchLoading }}
              disabled={selectedRowKeys.length === 0 || batchLoading}
            >
              <Button
                size="small"
                danger
                type="primary"
                icon={<DeleteOutlined />}
                disabled={selectedRowKeys.length === 0}
                loading={batchLoading}
              >
                批量删除
              </Button>
            </Popconfirm>
          </Space>
        </div>
      )}

      {screens.xs ? (
        <MobileCardList
          compact
          dataSource={filteredLevels}
          loading={loading}
          rowKey="id"
          pagination={false}
          renderCard={(record: any) => {
            const commPercent = Math.round((record.commission_ratio || 0) * 100);
            const commBasis = record.commission_basis === 'recharge' ? '充值' : '消费';
            return (
              <MobileCard
                compact
                title={
                  <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8 }}>
                    {isBatchEditMode && (
                      <Checkbox
                        checked={selectedRowKeys.includes(record.id)}
                        onClick={(e) => e.stopPropagation()}
                        style={{ marginTop: 2 }}
                        onChange={(e) => {
                          if (e.target.checked) {
                            setSelectedRowKeys((prev) => [...prev, record.id]);
                          } else {
                            setSelectedRowKeys((prev) => prev.filter((k) => k !== record.id));
                          }
                        }}
                      />
                    )}
                    <Space vertical size={3} align="start">
                      <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                        <TrophyOutlined className="text-muted-foreground text-xs" />
                        <Text strong style={{ fontSize: 13 }}>{record.name}</Text>
                      </div>
                      <span className="inline-flex items-center text-[11px] font-semibold px-1.5 py-0.5 rounded border border-border bg-muted/80 text-foreground">
                        ULID: {record.id.toString().padStart(4, '0')}
                      </span>
                      {record.is_default === 1 && (
                        <span className="inline-flex items-center text-[10px] px-1.5 py-0.5 rounded font-medium border border-border bg-foreground text-background">
                          默认注册
                        </span>
                      )}
                    </Space>
                  </div>
                }
                extra={null}
              >
                <CardRow label="用户数">
                  <span
                    className="inline-flex items-center text-xs px-2 py-0.5 rounded-full border border-border bg-muted/60 text-foreground font-medium hover:bg-muted transition-colors cursor-pointer select-none"
                    onClick={(e) => {
                      e.stopPropagation();
                      navigate(`/${adminPath}/users?group=${encodeURIComponent(record.group_key)}`);
                    }}
                  >
                    {record.user_count || 0}
                  </span>
                </CardRow>
                <CardRow label="用户等级折扣倍率">
                  {renderDiscountCell(record.discount, record.discount_type)}
                </CardRow>
                <CardRow label="等级购买">
                  {record.purchasable === 1 ? (
                    <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-border bg-muted/80 text-foreground font-medium">已开启</span>
                  ) : (
                    <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-transparent text-muted-foreground/60">已关闭</span>
                  )}
                </CardRow>
                <CardRow label="等级营销推广">
                  {record.marketing_enabled === 1 ? (
                    <Space vertical size={3} align="start">
                      <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-border bg-muted/80 text-foreground font-medium">已开启</span>
                      <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-border bg-muted/40 text-muted-foreground">{commPercent}% · {commBasis}</span>
                    </Space>
                  ) : (
                    <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-transparent text-muted-foreground/60">已关闭</span>
                  )}
                </CardRow>
                <CardRow label="详细日志">
                  {record.allow_view_log_details === 0 ? (
                    <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-transparent text-muted-foreground/60">已关闭</span>
                  ) : (
                    <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-border bg-muted/80 text-foreground font-medium">已开启</span>
                  )}
                </CardRow>
                <CardRow label="发票&收据">
                  {record.invoice_enabled === 1 ? (
                    <Space vertical size={3} align="start">
                      <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-border bg-muted/80 text-foreground font-medium">已开启</span>
                      <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-border bg-muted/40 text-muted-foreground">
                        {record.invoice_mode === 'china' ? '境内模式' : '国际模式'}
                      </span>
                    </Space>
                  ) : (
                    <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-transparent text-muted-foreground/60">已关闭</span>
                  )}
                </CardRow>
                <CardRow label="说明备注">
                  <Text
                    type="secondary"
                    style={{ fontSize: 12, cursor: 'pointer' }}
                    editable={{
                      text: record.description || '',
                      onChange: async (val) => {
                        if (val === record.description) return;
                        try {
                          await request.put(`/user_levels/${record.id}`, { description: val });
                          setLevels((prev) => prev.map((l) => (l.id === record.id ? { ...l, description: val } : l)));
                          message.success('说明备注已更新');
                        } catch (e: any) {
                          console.error(e);
                          message.error('修改说明备注失败');
                        }
                      },
                      tooltip: '点击修改说明备注',
                      triggerType: ['icon', 'text'],
                    }}
                  >
                    {record.description || <span style={{ opacity: 0.5, fontStyle: 'italic' }}>添加说明备注</span>}
                  </Text>
                </CardRow>
                <CardRow label="页面排序">
                  <div style={{ display: 'flex', alignItems: 'center' }}>
                    <Text
                      editable={{
                        tooltip: false,
                        text: String(record.sort_order ?? 0),
                        onChange: (val) => {
                          const num = parseInt(val.trim(), 10);
                          handleUpdateSortOrder(record.id, isNaN(num) ? 0 : num);
                        },
                      }}
                      style={{
                        fontSize: 12,
                        fontWeight: 600,
                        color: (record.sort_order ?? 0) > 0 ? '#1677ff' : undefined,
                      }}
                    >
                      {record.sort_order ?? 0}
                    </Text>
                  </div>
                </CardRow>
                <CardRow label="创建时间"><span className="text-xs text-muted-foreground">{formatApiDateTime(record.created_at, 'YYYY-MM-DD')}</span></CardRow>
                <CardActions>
                  <Button type="text" size="small" icon={<EditOutlined className="text-muted-foreground" />} onClick={() => handleEdit(record)} />
                  <Popconfirm title={t('user_levels.delete_confirm')} onConfirm={() => handleDelete(record.id)} disabled={isProtectedDefaultLevel(record)}>
                    <Button type="text" size="small" icon={<DeleteOutlined />} danger disabled={isProtectedDefaultLevel(record)} />
                  </Popconfirm>
                </CardActions>
              </MobileCard>
            );
          }}
        />
      ) : (
        <Table
          className="[&_.ant-table-tbody>tr>td]:!py-1.5 [&_.ant-table-thead>tr>th]:!py-1.5"
          dataSource={filteredLevels}
          columns={columns}
          rowKey="id"
          loading={loading}
          pagination={false}
          size="small"
          scroll={{ x: 'max-content' }}
          showSorterTooltip={false}
          rowSelection={isBatchEditMode ? {
            selectedRowKeys,
            onChange: (keys: React.Key[]) => setSelectedRowKeys(keys),
          } : undefined}
        />
      )}
    </>
  );

  if (embedded) return body;
  return <Card bordered={false}>{body}</Card>;
};

export default UserLevels;
