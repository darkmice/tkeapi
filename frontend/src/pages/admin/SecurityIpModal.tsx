/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useState, useMemo, useEffect } from 'react';
import {
  Modal,
  Table,
  Input,
  Button,
  Tag,
  Space,
  Typography,
  Tooltip,
  Popconfirm,
  Radio,
  Empty,
  Alert,
  message,
  theme,
} from 'antd';
import type { TableColumnsType } from 'antd';
import {
  SearchOutlined,
  PlusOutlined,
  DeleteOutlined,
  EditOutlined,
  CopyOutlined,
  ImportOutlined,
  ClearOutlined,
  CheckOutlined,
  CloseOutlined,
  SafetyCertificateOutlined,
  EyeOutlined,
  ReloadOutlined,
} from '@ant-design/icons';

const { Text, Paragraph } = Typography;
const { TextArea } = Input;

type IpRuleType = 'ipv4' | 'ipv4_cidr' | 'ipv6' | 'ipv6_cidr' | 'invalid';

interface IpRuleItem {
  id: string;
  raw: string;
  type: IpRuleType;
  scopeDesc: string;
  isValid: boolean;
}

export interface SecurityIpModalProps {
  open: boolean;
  type: 'blacklist' | 'whitelist';
  mode: 'view' | 'edit';
  initialList: string[];
  onClose: () => void;
  onSave?: (newList: string[]) => void;
  onSaveAndApply?: (newList: string[]) => Promise<boolean | void>;
  onSwitchToEdit?: () => void;
}

// 默认推荐内网段
const DEFAULT_WHITELIST_CIDRS = ['10.0.0.0/24', '192.168.1.0/24', '172.16.0.0/24'];

function isValidIpv4Part(part: string): boolean {
  if (!/^\d+$/.test(part)) return false;
  const n = parseInt(part, 10);
  if (n < 0 || n > 255) return false;
  if (part.length > 1 && part.startsWith('0')) return false;
  return true;
}

function isValidIpv4(ip: string): boolean {
  const parts = ip.split('.');
  if (parts.length !== 4) return false;
  return parts.every(isValidIpv4Part);
}

function parseIpRule(rawStr: string, index: number): IpRuleItem {
  const trimmed = rawStr.trim();
  const id = `${index}-${trimmed}`;

  if (!trimmed) {
    return { id, raw: '', type: 'invalid', scopeDesc: '空内容', isValid: false };
  }

  // 检测 IPv4 CIDR
  if (trimmed.includes('/') && !trimmed.includes(':')) {
    const [ipPart, maskPart] = trimmed.split('/');
    if (maskPart && /^\d+$/.test(maskPart)) {
      const mask = parseInt(maskPart, 10);
      if (mask >= 0 && mask <= 32 && isValidIpv4(ipPart)) {
        let countText = '';
        if (mask === 32) {
          countText = '1 个主机';
        } else if (mask === 0) {
          countText = '全网 IPv4 (42.9亿)';
        } else {
          const count = Math.pow(2, 32 - mask);
          countText = `${count.toLocaleString()} 个地址`;
        }
        return {
          id,
          raw: trimmed,
          type: 'ipv4_cidr',
          scopeDesc: countText,
          isValid: true,
        };
      }
    }
  }

  // 检测 IPv4 单主机
  if (isValidIpv4(trimmed)) {
    return {
      id,
      raw: trimmed,
      type: 'ipv4',
      scopeDesc: '1 个主机',
      isValid: true,
    };
  }

  // 检测 IPv6 CIDR
  if (trimmed.includes(':') && trimmed.includes('/')) {
    const [, maskPart] = trimmed.split('/');
    if (maskPart && /^\d+$/.test(maskPart)) {
      const mask = parseInt(maskPart, 10);
      if (mask >= 0 && mask <= 128) {
        return {
          id,
          raw: trimmed,
          type: 'ipv6_cidr',
          scopeDesc: `IPv6 子网 (/${mask})`,
          isValid: true,
        };
      }
    }
  }

  // 检测 IPv6 单地址
  if (trimmed.includes(':')) {
    return {
      id,
      raw: trimmed,
      type: 'ipv6',
      scopeDesc: '1 个主机',
      isValid: true,
    };
  }

  return {
    id,
    raw: trimmed,
    type: 'invalid',
    scopeDesc: '格式异常 (无法命中生效)',
    isValid: false,
  };
}

export const SecurityIpModal: React.FC<SecurityIpModalProps> = ({
  open,
  type,
  mode,
  initialList,
  onClose,
  onSave,
  onSaveAndApply,
  onSwitchToEdit,
}) => {
  const { token } = theme.useToken();

  const isBlacklist = type === 'blacklist';
  const titlePrefix = isBlacklist ? '注册 IP 黑名单' : '注册 IP 白名单';

  // 当前正在编辑的列表数据
  const [rules, setRules] = useState<string[]>([]);
  // 搜索关键字
  const [searchText, setSearchText] = useState('');
  // 分类过滤
  const [typeFilter, setTypeFilter] = useState<string>('all');
  // 批量选中的 keys
  const [selectedRowKeys, setSelectedRowKeys] = useState<React.Key[]>([]);

  // 单条录入输入框状态
  const [singleInput, setSingleInput] = useState('');
  const [singleInputError, setSingleInputError] = useState<string | null>(null);

  // 行内编辑状态
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingText, setEditingText] = useState('');

  // 批量导入抽屉/子对话框状态
  const [importModalOpen, setImportModalOpen] = useState(false);
  const [importText, setImportText] = useState('');
  const [importMode, setImportMode] = useState<'append' | 'overwrite'>('append');

  // 保存中状态
  const [saving, setSaving] = useState(false);

  // 初始化列表
  useEffect(() => {
    if (open) {
      setRules([...initialList]);
      setSearchText('');
      setTypeFilter('all');
      setSelectedRowKeys([]);
      setSingleInput('');
      setSingleInputError(null);
      setEditingId(null);
    }
  }, [open, initialList]);

  // 解析所有的规则条目
  const parsedItems: IpRuleItem[] = useMemo(() => {
    return rules.map((r, i) => parseIpRule(r, i));
  }, [rules]);

  // 筛选后的展示列表
  const filteredItems = useMemo(() => {
    return parsedItems.filter((item) => {
      // 关键字搜索
      if (searchText.trim()) {
        const query = searchText.trim().toLowerCase();
        if (!item.raw.toLowerCase().includes(query)) {
          return false;
        }
      }
      // 分类筛选
      if (typeFilter === 'ipv4' && item.type !== 'ipv4') return false;
      if (typeFilter === 'ipv4_cidr' && item.type !== 'ipv4_cidr') return false;
      if (typeFilter === 'ipv6' && item.type !== 'ipv6' && item.type !== 'ipv6_cidr') return false;
      if (typeFilter === 'invalid' && item.isValid) return false;
      return true;
    });
  }, [parsedItems, searchText, typeFilter]);

  // 统计概览
  const stats = useMemo(() => {
    let ipv4Count = 0;
    let cidrCount = 0;
    let ipv6Count = 0;
    let invalidCount = 0;
    for (const item of parsedItems) {
      if (!item.isValid) {
        invalidCount++;
      } else if (item.type === 'ipv4') {
        ipv4Count++;
      } else if (item.type === 'ipv4_cidr') {
        cidrCount++;
      } else if (item.type === 'ipv6' || item.type === 'ipv6_cidr') {
        ipv6Count++;
      }
    }
    return { total: parsedItems.length, ipv4Count, cidrCount, ipv6Count, invalidCount };
  }, [parsedItems]);

  // 单条添加规则
  const handleAddSingle = () => {
    const val = singleInput.trim();
    if (!val) {
      setSingleInputError('请输入 IP 或 CIDR 网段');
      return;
    }
    if (rules.includes(val)) {
      setSingleInputError('该条规则已存在，请勿重复添加');
      return;
    }
    const testItem = parseIpRule(val, 0);
    if (!testItem.isValid) {
      setSingleInputError('格式不符合 IPv4/IPv6 或 CIDR 规范');
      return;
    }

    setRules([val, ...rules]);
    setSingleInput('');
    setSingleInputError(null);
    message.success(`已添加规则：${val}`);
  };

  // 快捷填入白名单默认网段
  const handleApplyDefaultWhitelist = () => {
    const merged = Array.from(new Set([...rules, ...DEFAULT_WHITELIST_CIDRS]));
    setRules(merged);
    message.success('已填入默认推荐内网段');
  };

  // 一键复制全部
  const handleCopyAll = () => {
    if (rules.length === 0) {
      message.info('当前规则列表为空');
      return;
    }
    const content = rules.join('\n');
    navigator.clipboard
      .writeText(content)
      .then(() => {
        message.success(`已成功复制全部 ${rules.length} 条规则至剪贴板`);
      })
      .catch(() => {
        message.error('复制失败，请手动选择复制');
      });
  };

  // 单项删除
  const handleDeleteRow = (raw: string) => {
    const next = rules.filter((r) => r !== raw);
    setRules(next);
    setSelectedRowKeys(selectedRowKeys.filter((k) => !String(k).endsWith(`-${raw}`)));
    message.success('已删除');
  };

  // 批量删除选中的项
  const handleBatchDelete = () => {
    if (selectedRowKeys.length === 0) return;
    const selectedRaws = new Set(
      filteredItems.filter((item) => selectedRowKeys.includes(item.id)).map((item) => item.raw)
    );
    const next = rules.filter((r) => !selectedRaws.has(r));
    setRules(next);
    setSelectedRowKeys([]);
    message.success(`已成功删除选中的 ${selectedRaws.size} 条规则`);
  };

  // 清空全部
  const handleClearAll = () => {
    setRules([]);
    setSelectedRowKeys([]);
    message.success('已清空全部规则');
  };

  // 保存行内编辑
  const handleSaveEdit = (oldRaw: string) => {
    const val = editingText.trim();
    if (!val) {
      message.error('IP 或网段内容不能为空');
      return;
    }
    if (val !== oldRaw && rules.includes(val)) {
      message.error('目标规则已存在，请勿重复');
      return;
    }
    const idx = rules.indexOf(oldRaw);
    if (idx !== -1) {
      const next = [...rules];
      next[idx] = val;
      setRules(next);
      setEditingId(null);
      setEditingText('');
      message.success('已更新规则');
    }
  };

  // 批量解析导入
  const handleConfirmImport = () => {
    if (!importText.trim()) {
      message.warning('请输入要导入的内容');
      return;
    }
    // 支持按换行、分号、逗号、空格分割
    const lines = importText
      .split(/[\r\n,;\s]+/)
      .map((s) => s.trim())
      .filter(Boolean);

    if (lines.length === 0) {
      message.warning('未识别到有效条目');
      return;
    }

    if (importMode === 'overwrite') {
      const unique = Array.from(new Set(lines));
      setRules(unique);
      message.success(`已覆盖导入 ${unique.length} 条规则`);
    } else {
      const set = new Set(rules);
      let added = 0;
      for (const line of lines) {
        if (!set.has(line)) {
          set.add(line);
          added++;
        }
      }
      setRules(Array.from(set));
      message.success(`批量追加完成：新增 ${added} 条，已自动去重`);
    }

    setImportModalOpen(false);
    setImportText('');
  };

  // 确认并同步至父表单
  const handleApplyToForm = () => {
    if (onSave) {
      onSave(rules);
    }
    message.success('已同步至表单配置，点击页面底部「保存」即可提交生效');
    onClose();
  };

  // 确认并立即持久化保存
  const handleApplyAndSaveImmediately = async () => {
    if (!onSaveAndApply) {
      handleApplyToForm();
      return;
    }
    setSaving(true);
    try {
      if (onSave) {
        onSave(rules);
      }
      const res = await onSaveAndApply(rules);
      if (res !== false) {
        message.success('规则已成功保存并立即生效');
        onClose();
      }
    } catch {
      // 错误由请求全局拦截器弹出
    } finally {
      setSaving(false);
    }
  };

  // 渲染类型 Tag
  const renderTypeTag = (itemType: IpRuleType, isValid: boolean) => {
    if (!isValid || itemType === 'invalid') {
      return <Tag color="error">格式异常</Tag>;
    }
    switch (itemType) {
      case 'ipv4':
        return <Tag color="processing">IPv4 单地址</Tag>;
      case 'ipv4_cidr':
        return <Tag color="cyan">IPv4 CIDR</Tag>;
      case 'ipv6':
        return <Tag color="purple">IPv6 单地址</Tag>;
      case 'ipv6_cidr':
        return <Tag color="magenta">IPv6 CIDR</Tag>;
      default:
        return <Tag>未知</Tag>;
    }
  };

  // 表格列定义
  const columns: TableColumnsType<IpRuleItem> = [
    {
      title: '#',
      dataIndex: 'id',
      key: 'index',
      width: 60,
      align: 'center',
      render: (_: any, __: any, index: number) => (
        <Text type="secondary" style={{ fontSize: 12 }}>
          {index + 1}
        </Text>
      ),
    },
    {
      title: 'IP / CIDR 网段',
      dataIndex: 'raw',
      key: 'raw',
      render: (raw: string, record: IpRuleItem) => {
        if (mode === 'edit' && editingId === record.id) {
          return (
            <Space size={4}>
              <Input
                size="small"
                value={editingText}
                onChange={(e) => setEditingText(e.target.value)}
                onPressEnter={() => handleSaveEdit(raw)}
                style={{ width: 220 }}
                autoFocus
              />
              <Tooltip title="保存修改">
                <Button
                  size="small"
                  type="text"
                  icon={<CheckOutlined style={{ color: token.colorSuccess }} />}
                  onClick={() => handleSaveEdit(raw)}
                />
              </Tooltip>
              <Tooltip title="取消">
                <Button
                  size="small"
                  type="text"
                  icon={<CloseOutlined />}
                  onClick={() => {
                    setEditingId(null);
                    setEditingText('');
                  }}
                />
              </Tooltip>
            </Space>
          );
        }

        return (
          <Space size={6}>
            <Text code copyable={{ text: raw }} style={{ fontSize: 13, fontWeight: 500 }}>
              {raw}
            </Text>
          </Space>
        );
      },
    },
    {
      title: '规则类型',
      dataIndex: 'type',
      key: 'type',
      width: 130,
      render: (_: any, record: IpRuleItem) => renderTypeTag(record.type, record.isValid),
    },
    {
      title: '预估作用范围',
      dataIndex: 'scopeDesc',
      key: 'scopeDesc',
      width: 180,
      render: (scopeDesc: string, record: IpRuleItem) => (
        <Text type={record.isValid ? 'secondary' : 'danger'} style={{ fontSize: 12 }}>
          {scopeDesc}
        </Text>
      ),
    },
    ...(mode === 'edit'
      ? [
          {
            title: '操作',
            key: 'action',
            width: 100,
            align: 'center' as const,
            render: (_: any, record: IpRuleItem) => (
              <Space size={4}>
                <Tooltip title="编辑此条">
                  <Button
                    type="text"
                    size="small"
                    icon={<EditOutlined />}
                    onClick={() => {
                      setEditingId(record.id);
                      setEditingText(record.raw);
                    }}
                  />
                </Tooltip>
                <Tooltip title="删除">
                  <Popconfirm
                    title="确定移除该规则？"
                    okText="删除"
                    cancelText="取消"
                    okButtonProps={{ danger: true }}
                    onConfirm={() => handleDeleteRow(record.raw)}
                  >
                    <Button type="text" size="small" danger icon={<DeleteOutlined />} />
                  </Popconfirm>
                </Tooltip>
              </Space>
            ),
          },
        ]
      : []),
  ];

  return (
    <>
      <Modal
        open={open}
        title={
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, paddingBottom: 4 }}>
            <SafetyCertificateOutlined
              style={{
                fontSize: 20,
                color: isBlacklist ? token.colorError : token.colorSuccess,
              }}
            />
            <div>
              <div style={{ fontSize: 16, fontWeight: 600 }}>
                {mode === 'view' ? `查看明细：${titlePrefix}` : `配置 ${titlePrefix} 规则`}
              </div>
              <Text type="secondary" style={{ fontSize: 12, fontWeight: 'normal' }}>
                {isBlacklist
                  ? '拦截名单内的 IP 访问登录、注册与找回密码等高危接口。支持单个 IP 与 CIDR 网段。'
                  : '白名单内的 IP 免受黑名单和每日注册频次限制（优先于黑名单生效）。'}
              </Text>
            </div>
          </div>
        }
        width={880}
        onCancel={onClose}
        styles={{
          body: {
            padding: '16px 20px 8px',
            maxHeight: 'calc(85vh - 120px)',
            overflowY: 'auto',
          },
        }}
        footer={
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', width: '100%' }}>
            <div>
              <Text type="secondary" style={{ fontSize: 12 }}>
                共 <Text strong>{rules.length}</Text> 条规则
                {stats.invalidCount > 0 && (
                  <Text type="danger" style={{ marginLeft: 8 }}>
                    ({stats.invalidCount} 条格式异常)
                  </Text>
                )}
              </Text>
            </div>
            <Space>
              {mode === 'view' ? (
                <>
                  <Button icon={<CopyOutlined />} onClick={handleCopyAll}>
                    复制全部
                  </Button>
                  {onSwitchToEdit && (
                    <Button type="primary" icon={<EditOutlined />} onClick={onSwitchToEdit}>
                      切换到配置编辑
                    </Button>
                  )}
                  <Button onClick={onClose}>关闭</Button>
                </>
              ) : (
                <>
                  <Button onClick={onClose} disabled={saving}>
                    取消
                  </Button>
                  <Button onClick={handleApplyToForm} disabled={saving}>
                    同步至表单
                  </Button>
                  <Button type="primary" loading={saving} onClick={handleApplyAndSaveImmediately}>
                    保存并立即生效
                  </Button>
                </>
              )}
            </Space>
          </div>
        }
      >
        <Space orientation="vertical" size="middle" style={{ width: '100%' }}>
          {/* 编辑模式下的单条快速录入条 */}
          {mode === 'edit' && (
            <div
              style={{
                padding: '12px 14px',
                background: token.colorFillAlter,
                border: `1px solid ${token.colorBorderSecondary}`,
                borderRadius: 8,
              }}
            >
              <div style={{ display: 'flex', gap: 8, alignItems: 'flex-start' }}>
                <div style={{ flex: 1 }}>
                  <Input
                    placeholder="输入单个 IP 或 CIDR 网段，例如：192.168.1.100 或 10.0.0.0/8，按回车快速添加"
                    value={singleInput}
                    status={singleInputError ? 'error' : ''}
                    onChange={(e) => {
                      setSingleInput(e.target.value);
                      if (singleInputError) setSingleInputError(null);
                    }}
                    onPressEnter={handleAddSingle}
                    allowClear
                  />
                  {singleInputError && (
                    <div style={{ marginTop: 4 }}>
                      <Text type="danger" style={{ fontSize: 12 }}>
                        {singleInputError}
                      </Text>
                    </div>
                  )}
                </div>
                <Button type="primary" icon={<PlusOutlined />} onClick={handleAddSingle}>
                  添加规则
                </Button>
              </div>
            </div>
          )}

          {/* 工具栏与过滤栏 */}
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              flexWrap: 'wrap',
              gap: 8,
            }}
          >
            {/* 左侧搜索与分类过滤 */}
            <Space size="middle" wrap>
              <Input
                placeholder="快速搜索 IP / 网段..."
                prefix={<SearchOutlined style={{ color: token.colorTextSecondary }} />}
                value={searchText}
                onChange={(e) => setSearchText(e.target.value)}
                allowClear
                style={{ width: 200 }}
              />
              <Radio.Group
                size="small"
                value={typeFilter}
                onChange={(e) => setTypeFilter(e.target.value)}
                optionType="button"
                buttonStyle="solid"
              >
                <Radio.Button value="all">全部 ({stats.total})</Radio.Button>
                <Radio.Button value="ipv4">单 IP ({stats.ipv4Count})</Radio.Button>
                <Radio.Button value="ipv4_cidr">CIDR 网段 ({stats.cidrCount})</Radio.Button>
                {stats.ipv6Count > 0 && <Radio.Button value="ipv6">IPv6 ({stats.ipv6Count})</Radio.Button>}
                {stats.invalidCount > 0 && (
                  <Radio.Button value="invalid">异常 ({stats.invalidCount})</Radio.Button>
                )}
              </Radio.Group>
            </Space>

            {/* 右侧批量与功能按钮 */}
            <Space size="small" wrap>
              {selectedRowKeys.length > 0 && mode === 'edit' && (
                <Popconfirm
                  title={`确认批量删除选中的 ${selectedRowKeys.length} 条规则？`}
                  onConfirm={handleBatchDelete}
                  okText="删除"
                  cancelText="取消"
                  okButtonProps={{ danger: true }}
                >
                  <Button danger size="small" icon={<DeleteOutlined />}>
                    批量删除 ({selectedRowKeys.length})
                  </Button>
                </Popconfirm>
              )}

              {mode === 'edit' && (
                <>
                  <Button
                    size="small"
                    icon={<ImportOutlined />}
                    onClick={() => setImportModalOpen(true)}
                  >
                    批量导入
                  </Button>
                  {!isBlacklist && (
                    <Tooltip title="一键将 10.0.0.0/24、192.168.1.0/24、172.16.0.0/24 追加至当前白名单">
                      <Button size="small" icon={<ReloadOutlined />} onClick={handleApplyDefaultWhitelist}>
                        填入推荐内网段
                      </Button>
                    </Tooltip>
                  )}
                  {rules.length > 0 && (
                    <Popconfirm
                      title="确定要清空全部规则？此操作不可逆！"
                      onConfirm={handleClearAll}
                      okText="确认清空"
                      cancelText="取消"
                      okButtonProps={{ danger: true }}
                    >
                      <Button size="small" danger icon={<ClearOutlined />}>
                        清空
                      </Button>
                    </Popconfirm>
                  )}
                </>
              )}

              <Button size="small" icon={<CopyOutlined />} onClick={handleCopyAll}>
                一键复制全部
              </Button>
            </Space>
          </div>

          {/* 表格区 */}
          <Table
            size="small"
            rowKey="id"
            columns={columns}
            dataSource={filteredItems}
            rowSelection={
              mode === 'edit'
                ? {
                    selectedRowKeys,
                    onChange: (keys) => setSelectedRowKeys(keys),
                  }
                : undefined
            }
            pagination={{
              defaultPageSize: 10,
              pageSizeOptions: ['10', '20', '50', '100', '200'],
              showSizeChanger: true,
              showQuickJumper: true,
              showTotal: (total) => `共 ${total} 条规则`,
            }}
            locale={{
              emptyText: (
                <Empty
                  image={Empty.PRESENTED_IMAGE_SIMPLE}
                  description={
                    searchText ? (
                      <span>未匹配到符合 "{searchText}" 的规则</span>
                    ) : (
                      <span>暂未配置任何规则</span>
                    )
                  }
                />
              ),
            }}
          />
        </Space>
      </Modal>

      {/* 批量导入子对话框 */}
      <Modal
        open={importModalOpen}
        title={
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <ImportOutlined style={{ color: token.colorPrimary }} />
            <span>批量导入 IP / CIDR 网段规则</span>
          </div>
        }
        width={620}
        onCancel={() => {
          setImportModalOpen(false);
          setImportText('');
        }}
        onOk={handleConfirmImport}
        okText="确认导入"
        cancelText="取消"
      >
        <Space orientation="vertical" size="small" style={{ width: '100%' }}>
          <Alert
            type="info"
            showIcon
            message="支持海量数据一次性粘贴。支持每行一条，亦支持英文逗号、中文逗号、分号或空格分隔。"
          />
          <div style={{ margin: '8px 0 4px' }}>
            <Text strong>导入方式：</Text>
            <Radio.Group
              style={{ marginLeft: 10 }}
              value={importMode}
              onChange={(e) => setImportMode(e.target.value)}
            >
              <Radio value="append">追加到现有列表 (自动去重)</Radio>
              <Radio value="overwrite">覆盖当前列表</Radio>
            </Radio.Group>
          </div>
          <TextArea
            rows={10}
            placeholder={`示例：\n192.168.1.100\n10.0.0.0/8\n172.16.0.0/16\n2001:db8::/32`}
            value={importText}
            onChange={(e) => setImportText(e.target.value)}
            style={{ fontFamily: 'monospace', fontSize: 13 }}
          />
          <Paragraph type="secondary" style={{ fontSize: 12, margin: 0 }}>
            粘贴后系统将自动剔除空行并校验格式，异常或重复条目将给出友好统计。
          </Paragraph>
        </Space>
      </Modal>
    </>
  );
};
