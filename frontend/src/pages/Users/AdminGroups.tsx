/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useState, useEffect, useMemo } from 'react';
import { Table, Button, Space, message, Card, Typography, Grid, Popconfirm, Input, Checkbox } from 'antd';
import MobileCardList, { MobileCard, CardRow, CardActions } from '../../components/MobileCardList';
import { PlusOutlined, EditOutlined, DeleteOutlined, SafetyCertificateOutlined, CheckSquareOutlined } from '@ant-design/icons';
import { useNavigate } from 'react-router-dom';
import request from '../../utils/request';
import { fetchActivePlugins } from '../../utils/activePlugins';
import { formatApiDateTime } from '../../utils/timedisplay';
import useSettingsStore from '../../store/settings';
import { useTranslation } from 'react-i18next';
import type { AdminGroup } from '../../types';
import {
  ADMIN_MENU_PERMISSIONS,
  expandLegacyAdminMenuPermissions,
  flattenAdminMenuPermissions,
  getAdminMenuPermissionLabel,
  parseAdminGroupPermissions,
} from '../../constants/adminMenuPermissions';

const { Title, Text } = Typography;
const { useBreakpoint } = Grid;

const ALL_BASIC_PERMISSION_VALUES = flattenAdminMenuPermissions();

interface AdminGroupsProps {
  embedded?: boolean;
  keyword?: string;
  isBatchEditMode?: boolean;
  onBatchEditModeChange?: (active: boolean) => void;
}

const AdminGroups: React.FC<AdminGroupsProps> = ({
  embedded = false,
  keyword,
  isBatchEditMode: controlledBatchEdit,
  onBatchEditModeChange,
}) => {
  const [groups, setGroups] = useState<AdminGroup[]>([]);
  const [loading, setLoading] = useState(false);
  const [activePlugins, setActivePlugins] = useState<any[]>([]);
  const [searchKeyword, setSearchKeyword] = useState('');
  const [internalBatchEdit, setInternalBatchEdit] = useState(false);
  const [selectedRowKeys, setSelectedRowKeys] = useState<React.Key[]>([]);
  const [batchLoading, setBatchLoading] = useState(false);
  const screens = useBreakpoint();
  const navigate = useNavigate();
  const { t } = useTranslation();
  const { settings } = useSettingsStore();
  const adminPath = settings?.site?.admin_path || 'admin1688';

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

  const fetchGroups = async () => {
    setLoading(true);
    try {
      const response = await (request.get('/admin_groups') as any);
      setGroups(response.data);
    } catch (error) {
      console.error(error);
    } finally {
      setLoading(false);
    }
  };

  const loadActivePlugins = async () => {
    try {
      const response = await fetchActivePlugins();
      if (response.active_plugins) {
        setActivePlugins(response.active_plugins);
      }
    } catch (error) {
      console.error('获取插件失败', error);
    }
  };

  useEffect(() => {
    fetchGroups();
    loadActivePlugins();
  }, []);

  const activeKeyword = keyword !== undefined ? keyword : searchKeyword;

  const filteredGroups = useMemo(() => {
    if (!activeKeyword.trim()) return groups;
    const kw = activeKeyword.trim().toLowerCase();
    return groups.filter((g) => {
      const matchName = g.name?.toLowerCase().includes(kw);
      const matchId = g.id?.toString().includes(kw);
      const matchUlid = `ulid: ${g.id.toString().padStart(4, '0')}`.toLowerCase().includes(kw) ||
                         g.id.toString().padStart(4, '0').includes(kw);
      const matchDesc = g.description?.toLowerCase().includes(kw);
      return matchName || matchId || matchUlid || matchDesc;
    });
  }, [groups, activeKeyword]);

  const handleCreate = () => {
    navigate(`/${adminPath}/admin-groups/new`);
  };

  const handleEdit = (group: AdminGroup) => {
    navigate(`/${adminPath}/admin-groups/${group.id}`);
  };

  const handleDelete = async (id: number) => {
    try {
      await request.delete(`/admin_groups/${id}`);
      message.success('删除成功');
      setSelectedRowKeys((prev) => prev.filter((k) => k !== id));
      fetchGroups();
    } catch (error: any) {
      console.error(error);
      message.error(error?.message || '删除失败');
    }
  };

  const handleBatchDelete = async () => {
    if (selectedRowKeys.length === 0) return;
    const selectedKeySet = new Set(selectedRowKeys.map((k) => Number(k)));
    const targetGroups = groups.filter((g) => selectedKeySet.has(g.id) && (g.user_count || 0) === 0);
    if (targetGroups.length === 0) {
      message.warning('选中的等级下仍有管理员，无法删除');
      return;
    }
    setBatchLoading(true);
    try {
      await Promise.all(
        targetGroups.map((g) => request.delete(`/admin_groups/${g.id}`))
      );
      message.success(`已成功删除 ${targetGroups.length} 个管理员等级`);
      setSelectedRowKeys([]);
      fetchGroups();
    } catch (e: any) {
      console.error(e);
      message.error(e?.message || '批量删除失败');
      fetchGroups();
    } finally {
      setBatchLoading(false);
    }
  };

  const renderPermissionsTags = (permissionsStr?: string) => {
    const policy = parseAdminGroupPermissions(permissionsStr);
    const perms = policy.view;
    const basicPerms = perms.filter((p) => !p.startsWith('plugin:'));
    const expandedBasic = expandLegacyAdminMenuPermissions(basicPerms);
    const hasAllBasic = ALL_BASIC_PERMISSION_VALUES.every((item) => expandedBasic.includes(item));
    const hasAllPlugins =
      activePlugins.length === 0 ||
      activePlugins.every((plugin) => perms.includes(`plugin:${plugin.name}`));

    const hasFullEdit = perms.length > 0 && perms.every((p) => policy.edit.includes(p));
    if (hasAllBasic && hasAllPlugins) {
      return (
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: '4px' }}>
          <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-border bg-foreground text-background font-medium">
            {t('admin_perm.all_features')}
          </span>
          {!hasFullEdit && (
            <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-border bg-muted/40 text-muted-foreground">
              {t('admin_perm.partial_readonly')}
            </span>
          )}
        </div>
      );
    }

    const displayKeys: string[] = [];
    for (const group of ADMIN_MENU_PERMISSIONS) {
      if (!group.children?.length) {
        if (expandedBasic.includes(group.value)) displayKeys.push(group.value);
        continue;
      }
      const selectedChildren = group.children.filter((c) => expandedBasic.includes(c.value));
      if (selectedChildren.length === 0) continue;
      if (selectedChildren.length === group.children.length) {
        displayKeys.push(group.value);
      } else {
        selectedChildren.forEach((c) => displayKeys.push(c.value));
      }
    }
    perms.filter((p) => p.startsWith('plugin:')).forEach((p) => displayKeys.push(p));

    const permLabels = displayKeys.map((p) => {
      if (p.startsWith('plugin:')) {
        const pName = p.substring(7);
        const foundPlugin = activePlugins.find((ap) => ap.name === pName);
        return `${t('admin_perm.plugin_prefix')}${foundPlugin ? t(`plugin_titles.${pName}`, foundPlugin.title || pName) : pName}`;
      }
      return getAdminMenuPermissionLabel(p, t);
    });

    if (permLabels.length === 0) {
      return <Text type="secondary" style={{ fontSize: '11px' }}>{t('admin_perm.none')}</Text>;
    }

    return (
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: '4px' }}>
        {permLabels.map((label) => (
          <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-border bg-muted/40 text-muted-foreground" key={label}>
            {label}
          </span>
        ))}
        {!hasFullEdit && perms.length > 0 && (
          <span className="inline-flex items-center text-[11px] px-1.5 py-0.5 rounded border border-border bg-muted/30 text-muted-foreground">
            {t('admin_perm.partial_readonly')}
          </span>
        )}
      </div>
    );
  };

  const columns = [
    {
      title: '管理员等级ULID',
      dataIndex: 'id',
      key: 'id',
      width: 170,
      sorter: (a: AdminGroup, b: AdminGroup) => a.id - b.id,
      render: (_: any, record: AdminGroup) => (
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
                    await request.put(`/admin_groups/${record.id}`, { description: val });
                    setGroups((prev) => prev.map((g) => (g.id === record.id ? { ...g, description: val } : g)));
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
      title: '名称', 
      dataIndex: 'name', 
      key: 'name', 
      width: 160, 
      sorter: (a: AdminGroup, b: AdminGroup) => a.name.localeCompare(b.name, 'zh'),
      render: (text: string) => (
        <div style={{ display: 'flex', alignItems: 'center', gap: 5 }}>
          <SafetyCertificateOutlined className="text-muted-foreground text-xs" />
          <Text strong style={{ fontSize: 13, lineHeight: '18px' }}>{text}</Text>
        </div>
      ),
    },
    {
      title: '用户数',
      dataIndex: 'user_count',
      key: 'user_count',
      width: 80,
      sorter: (a: AdminGroup, b: AdminGroup) => (a.user_count || 0) - (b.user_count || 0),
      render: (val: number, record: AdminGroup) => (
        <span
          className="inline-flex items-center text-xs px-2 py-0.5 rounded-full border border-border bg-muted/60 text-foreground font-medium hover:bg-muted transition-colors cursor-pointer select-none"
          onClick={(e) => {
            e.stopPropagation();
            navigate(`/${adminPath}/admins?group=${record.id}`);
          }}
        >
          {val || 0}
        </span>
      ),
    },
    { 
      title: '权限详细', 
      key: 'permissions_detail', 
      width: 500,
      render: (_: any, record: AdminGroup) => renderPermissionsTags(record.permissions)
    },
    {
      title: '排序',
      dataIndex: 'sort_order',
      key: 'sort_order',
      width: 80,
      sorter: (a: AdminGroup, b: AdminGroup) => (a.sort_order || 0) - (b.sort_order || 0),
      render: (val: number) => <span className="text-xs text-muted-foreground">{val || 0}</span>,
    },
    { 
      title: '创建时间', 
      dataIndex: 'created_at', 
      key: 'created_at', 
      width: 120,
      sorter: (a: AdminGroup, b: AdminGroup) => {
        const timeA = a.created_at ? new Date(a.created_at).getTime() : 0;
        const timeB = b.created_at ? new Date(b.created_at).getTime() : 0;
        return timeA - timeB;
      },
      render: (text: string) => <span className="text-xs text-muted-foreground">{formatApiDateTime(text, 'YYYY-MM-DD')}</span>,
    },
    { 
      title: t('common.actions', '操作'), 
      key: 'action', 
      width: 90,
      render: (_: any, record: AdminGroup) => (
        <Space size={2}>
          <Button type="text" size="small" icon={<EditOutlined className="text-muted-foreground" />} onClick={() => handleEdit(record)} />
          <Popconfirm
            title="确认删除该管理员等级？"
            onConfirm={() => handleDelete(record.id)}
            disabled={(record.user_count || 0) > 0}
          >
            <Button
              type="text"
              size="small"
              icon={<DeleteOutlined />}
              danger
              disabled={(record.user_count || 0) > 0}
            />
          </Popconfirm>
        </Space>
      ),
    },
  ];

  const body = (
    <>
      {!embedded && (
      <div style={{ display: 'flex', flexDirection: screens.xs ? 'column' : 'row', justifyContent: 'space-between', marginBottom: 12, gap: 12 }}>
        <Title level={screens.xs ? 4 : 2} style={{ margin: 0 }}>管理员等级</Title>
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
          <Button type="primary" icon={<PlusOutlined />} onClick={handleCreate}>
            添加管理员等级
          </Button>
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
                const selectableKeys = filteredGroups
                  .filter((g) => !g.user_count || g.user_count === 0)
                  .map((g) => g.id);
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
            <Popconfirm
              title="确定要批量删除选中的管理员等级吗？"
              description={`将一次性删除选中的 ${selectedRowKeys.length} 个管理员等级，此操作不可撤销。有管理员绑定的等级不可删除。`}
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
          dataSource={filteredGroups}
          loading={loading}
          rowKey="id"
          pagination={false}
          renderCard={(record: any) => (
            <MobileCard
              compact
              title={
                <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8 }}>
                  {isBatchEditMode && (
                    <Checkbox
                      checked={selectedRowKeys.includes(record.id)}
                      disabled={(record.user_count || 0) > 0}
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
                      <SafetyCertificateOutlined className="text-muted-foreground text-xs" />
                      <Text strong style={{ fontSize: 13 }}>{record.name}</Text>
                    </div>
                    <span className="inline-flex items-center text-[11px] font-semibold px-1.5 py-0.5 rounded border border-border bg-muted/80 text-foreground">
                      ULID: {record.id.toString().padStart(4, '0')}
                    </span>
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
                    navigate(`/${adminPath}/admins?group=${record.id}`);
                  }}
                >
                  {record.user_count || 0}
                </span>
              </CardRow>
              <CardRow label="权限详细">{renderPermissionsTags(record.permissions)}</CardRow>
              <CardRow label="说明备注">
                <Text
                  type="secondary"
                  style={{ fontSize: 12, cursor: 'pointer' }}
                  editable={{
                    text: record.description || '',
                    onChange: async (val) => {
                      if (val === record.description) return;
                      try {
                        await request.put(`/admin_groups/${record.id}`, { description: val });
                        setGroups((prev) => prev.map((g) => (g.id === record.id ? { ...g, description: val } : g)));
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
              <CardRow label="排序"><span className="text-xs text-muted-foreground">{record.sort_order || 0}</span></CardRow>
              <CardRow label="创建时间"><span className="text-xs text-muted-foreground">{formatApiDateTime(record.created_at, 'YYYY-MM-DD')}</span></CardRow>
              <CardActions>
                <Button type="text" size="small" icon={<EditOutlined className="text-muted-foreground" />} onClick={() => handleEdit(record)} />
                <Popconfirm
                  title="确认删除该管理员等级？"
                  onConfirm={() => handleDelete(record.id)}
                  disabled={(record.user_count || 0) > 0}
                >
                  <Button
                    type="text"
                    size="small"
                    icon={<DeleteOutlined />}
                    danger
                    disabled={(record.user_count || 0) > 0}
                  />
                </Popconfirm>
              </CardActions>
            </MobileCard>
          )}
        />
      ) : (
        <Table 
          className="[&_.ant-table-tbody>tr>td]:!py-1.5 [&_.ant-table-thead>tr>th]:!py-1.5"
          columns={columns} 
          dataSource={filteredGroups} 
          rowKey="id" 
          loading={loading}
          pagination={false}
          size="small"
          scroll={{ x: 'max-content' }}
          showSorterTooltip={false}
          rowSelection={isBatchEditMode ? {
            selectedRowKeys,
            onChange: (keys: React.Key[]) => setSelectedRowKeys(keys),
            getCheckboxProps: (record: AdminGroup) => ({
              disabled: (record.user_count || 0) > 0,
            }),
          } : undefined}
        />
      )}
    </>
  );

  if (embedded) return body;
  return <Card bordered={false}>{body}</Card>;
};

export default AdminGroups;
