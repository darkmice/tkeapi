import { useCallback, useEffect, useState } from 'react';
import { Button, Descriptions, Input, Modal, Popconfirm, Select, Space, Table, Tag, Tooltip, Typography, App } from 'antd';
import { EyeOutlined, RollbackOutlined, StopOutlined } from '@ant-design/icons';
import request from '../../utils/request';
import { formatApiDateTime } from '../../utils/timedisplay';

type CouponRow = {
  source: 'purchase' | 'redeem' | 'balance' | 'playground_space';
  id: number;
  order_id: number | null;
  name: string;
  face_value: number;
  worth_value?: number;
  credit_discount?: number;
  model_discounts?: Record<string, number>;
  system_amount?: number;
  gift_amount?: number;
  remaining: number;
  models: string[];
  expires_at?: string | null;
  status: string;
  created_at?: string | null;
  pay_method: string;
  code: string;
  voidable: boolean;
  redemption_id?: number | null;
  log_id?: number | null;
  space_mb?: number;
};

type PlanOption = {
  id: number;
  name: string;
  offer_kind?: string;
  price_month?: number | null;
  price_quarter?: number | null;
  price_year?: number | null;
};

function grantCycles(plan?: PlanOption) {
  if (!plan || plan.offer_kind !== 'subscription') return [];
  const items: { value: string; label: string }[] = [];
  if (plan.price_month != null) items.push({ value: 'month', label: '按月 · 30 天' });
  if (plan.price_quarter != null) items.push({ value: 'quarter', label: '按季 · 90 天' });
  if (plan.price_year != null) items.push({ value: 'year', label: '按年 · 365 天' });
  return items;
}

const STATUS_TEXT: Record<string, string> = {
  active: '可用',
  exhausted: '已用完',
  expired: '已过期',
  voided: '已作废',
  credited: '已入余额',
  returned: '已退换',
};

function errText(err: any, fallback: string) {
  return err?.response?.data?.error?.message || err?.message || fallback;
}

export default function UserCouponPanel({ userId }: { userId: string }) {
  const { message } = App.useApp();
  const [loading, setLoading] = useState(false);
  const [rows, setRows] = useState<CouponRow[]>([]);
  const [plans, setPlans] = useState<PlanOption[]>([]);
  const [grantOpen, setGrantOpen] = useState(false);
  const [planId, setPlanId] = useState<number | null>(null);
  const [grantCycle, setGrantCycle] = useState('');
  const [granting, setGranting] = useState(false);
  const [redeemOpen, setRedeemOpen] = useState(false);
  const [code, setCode] = useState('');
  const [redeeming, setRedeeming] = useState(false);
  const [detail, setDetail] = useState<CouponRow | null>(null);

  const load = useCallback(async () => {
    if (!userId) return;
    setLoading(true);
    try {
      const res = (await request.get(`/plugins/marketing_membership/admin/users/${encodeURIComponent(userId)}/coupons`)) as {
        coupons?: CouponRow[];
        plans?: PlanOption[];
      };
      const coupons = res.coupons || [];
      coupons.sort((a, b) => String(b.created_at || '').localeCompare(String(a.created_at || '')));
      setRows(coupons);
      setPlans(res.plans || []);
    } catch (err: any) {
      message.error(errText(err, '卡券记录加载失败'));
    } finally {
      setLoading(false);
    }
  }, [userId, message]);

  useEffect(() => {
    load();
  }, [load]);

  const voidRow = async (row: CouponRow) => {
    try {
      if (row.source === 'redeem') {
        await request.post(`/plugins/marketing_membership/admin/vouchers/${row.id}/void`);
      } else if (row.order_id) {
        await request.post(`/plugins/marketing_membership/admin/orders/${row.order_id}/void`);
      }
      message.success('已作废');
      load();
    } catch (err: any) {
      message.error(errText(err, '作废失败'));
    }
  };

  const grant = async () => {
    if (!planId) {
      message.warning('请选择要发放的方案');
      return;
    }
    setGranting(true);
    try {
      const plan = plans.find((item) => item.id === planId);
      const cycles = grantCycles(plan);
      await request.post(`/plugins/marketing_membership/admin/users/${encodeURIComponent(userId)}/grant`, {
        plan_id: planId,
        billing_cycle: cycles.length ? (grantCycle || cycles[0].value) : '',
      });
      message.success('已发放');
      setGrantOpen(false);
      setPlanId(null);
      setGrantCycle('');
      load();
    } catch (err: any) {
      message.error(errText(err, '发放失败'));
    } finally {
      setGranting(false);
    }
  };

  const revokeSpace = async (row: CouponRow) => {
    if (!row.redemption_id) return;
    try {
      await request.post(
        `/redemptions/${row.redemption_id}/revoke-space`,
        row.log_id ? { log_id: row.log_id } : undefined,
      );
      message.success('已收回存储空间');
      load();
    } catch (err: any) {
      message.error(errText(err, '收回失败'));
    }
  };

  const refundBalance = async (row: CouponRow) => {
    if (!row.redemption_id) return;
    try {
      await request.post(
        `/redemptions/${row.redemption_id}/refund-balance`,
        row.log_id ? { log_id: row.log_id } : undefined,
      );
      message.success('已退还余额');
      load();
    } catch (err: any) {
      message.error(errText(err, '退还失败'));
    }
  };

  const redeem = async () => {
    const next = code.trim();
    if (!next) {
      message.warning('请输入兑换码');
      return;
    }
    setRedeeming(true);
    try {
      await request.post(`/plugins/marketing_membership/admin/users/${encodeURIComponent(userId)}/redeem`, { code: next });
      message.success('已帮用户兑换');
      setRedeemOpen(false);
      setCode('');
      load();
    } catch (err: any) {
      message.error(errText(err, '兑换失败'));
    } finally {
      setRedeeming(false);
    }
  };

  const grantPlan = plans.find((plan) => plan.id === planId);
  const cycles = grantCycles(grantPlan);

  return (
    <div>
      <Space style={{ marginBottom: 8 }}>
        <Button htmlType="button" type="primary" size="small" onClick={() => setGrantOpen(true)}>发放卡券</Button>
        <Button htmlType="button" size="small" onClick={() => setRedeemOpen(true)}>帮用户兑换</Button>
      </Space>
      <Typography.Text type="secondary" style={{ display: 'block', marginBottom: 8, fontSize: 12, lineHeight: 1.4 }}>
        作废只停止这张卡还没用完的额度和同一订单带上的空间、等级，不退购买款，也不把面值退进余额。
      </Typography.Text>
      <Table
        size="small"
        rowKey={(row) => `${row.source}-${row.order_id || 0}-${row.id}-${row.code}-${row.created_at || ''}`}
        loading={loading}
        dataSource={rows}
        pagination={false}
        scroll={{ x: 1040 }}
        locale={{ emptyText: '还没有卡券记录' }}
        columns={[
          {
            title: '来源',
            width: 120,
            render: (_, row) => {
              if (row.source === 'balance') return <Tag color="orange">余额兑换</Tag>;
              if (row.source === 'playground_space') return <Tag color="cyan">创作空间</Tag>;
              if (row.source === 'redeem') return <Tag>兑换码</Tag>;
              if (row.pay_method === 'admin') return <Tag color="blue">管理员发放</Tag>;
              return <Tag color="green">用户购买</Tag>;
            },
          },
          { title: '名称', dataIndex: 'name', ellipsis: true },
          {
            title: '面值 / 剩余',
            width: 155,
            render: (_, row) => {
              if (row.source === 'balance') {
                const sys = Number(row.system_amount || 0);
                const gift = Number(row.gift_amount || 0);
                if (gift > 0) {
                  return sys > 0 ? `${row.face_value} (系统 ${sys} / 赠 ${gift})` : `${row.face_value} (赠送金)`;
                }
                return String(row.face_value);
              }
              if (row.source === 'playground_space') return `${row.space_mb || 0} MB`;
              if (row.face_value > 0) {
                const worth = Number(row.worth_value || 0);
                return worth > row.face_value
                  ? `${row.remaining} / ${row.face_value} (价值 ${worth})`
                  : `${row.remaining} / ${row.face_value}`;
              }
              return '—';
            },
          },
          {
            title: '兑换码',
            width: 180,
            render: (_, row) => row.code ? (
              <Typography.Text copyable={{ text: row.code }} className="font-mono text-xs">{row.code}</Typography.Text>
            ) : '—',
          },
          {
            title: '状态',
            width: 90,
            dataIndex: 'status',
            render: (status: string) => (
              <Tag color={status === 'active' ? 'success' : status === 'voided' ? 'error' : status === 'credited' ? 'orange' : 'default'}>
                {STATUS_TEXT[status] || status}
              </Tag>
            ),
          },
          {
            title: '到期',
            width: 170,
            render: (_, row) => {
              if (row.source === 'balance') return '—';
              return row.expires_at ? formatApiDateTime(row.expires_at) : '长期';
            },
          },
          {
            title: '时间',
            width: 210,
            render: (_, row) => (
              <span style={{ whiteSpace: 'nowrap' }}>{row.created_at ? formatApiDateTime(row.created_at) : '—'}</span>
            ),
          },
          {
            title: '操作',
            width: 88,
            fixed: 'right' as const,
            render: (_, row) => (
              <Space size={0}>
                <Tooltip title="详细">
                  <Button htmlType="button" type="text" size="small" icon={<EyeOutlined />} onClick={() => setDetail(row)} />
                </Tooltip>
                {row.source === 'playground_space' && row.status === 'active' && row.redemption_id ? (
                  <Popconfirm
                    placement="topRight"
                    title="收回这次空间？"
                    description={(
                      <span style={{ display: 'block', width: 220, whiteSpace: 'normal' }}>
                        只收回这一次增加的创作中心存储空间。兑换码不关闭，已用次数不退回。已上传的文件不删除。
                      </span>
                    )}
                    okText="收回"
                    cancelText="取消"
                    okButtonProps={{ danger: true }}
                    onConfirm={() => revokeSpace(row)}
                  >
                    <Tooltip title="收回空间">
                      <Button htmlType="button" type="text" danger size="small" icon={<RollbackOutlined />} />
                    </Tooltip>
                  </Popconfirm>
                ) : null}
                {row.source === 'balance' && row.status !== 'returned' && row.redemption_id ? (
                  <Popconfirm
                    placement="topRight"
                    title="退还余额？"
                    description={(
                      <span style={{ display: 'block', width: 220, whiteSpace: 'normal' }}>
                        {row.log_id
                          ? '只扣回这一次入账。这个人的其它次数和其它兑换用户不动，兑换码也不关闭。已消费时余额可以为负。这一次的兑换次数不退回。'
                          : '将从兑换用户的系统余额扣回已入账金额。用户已消费时余额可以为负。扣回后此码不可再兑换。'}
                      </span>
                    )}
                    okText="退还"
                    cancelText="取消"
                    okButtonProps={{ danger: true }}
                    onConfirm={() => refundBalance(row)}
                  >
                    <Tooltip title="退还余额">
                      <Button htmlType="button" type="text" danger size="small" icon={<RollbackOutlined />} />
                    </Tooltip>
                  </Popconfirm>
                ) : null}
                {row.voidable ? (
                  <Popconfirm title="作废这张卡券？剩余额度立即停用，不退款。" onConfirm={() => voidRow(row)}>
                    <Tooltip title="作废">
                      <Button htmlType="button" type="text" danger size="small" icon={<StopOutlined />} />
                    </Tooltip>
                  </Popconfirm>
                ) : null}
              </Space>
            ),
          },
        ]}
      />
      <Modal
        title={detail?.name || '卡券详细'}
        open={!!detail}
        onCancel={() => setDetail(null)}
        footer={null}
        width={440}
        destroyOnHidden
      >
        {detail ? (
          <Descriptions size="small" column={1} bordered>
            <Descriptions.Item label="来源">
              {detail.source === 'balance'
                ? '余额兑换'
                : detail.source === 'playground_space'
                  ? '创作空间兑换'
                  : detail.source === 'redeem'
                    ? '兑换码'
                    : detail.pay_method === 'admin'
                      ? '管理员发放'
                      : '用户购买'}
            </Descriptions.Item>
            <Descriptions.Item label="名称">{detail.name || '—'}</Descriptions.Item>
            <Descriptions.Item label={detail.source === 'playground_space' ? '存储空间' : detail.source === 'balance' ? '面值' : '剩余 / 面值'}>
              {detail.source === 'playground_space'
                ? `${detail.space_mb || 0} MB`
                : detail.source === 'balance'
                  ? (() => {
                    const sys = Number(detail.system_amount || 0);
                    const gift = Number(detail.gift_amount || 0);
                    if (gift > 0) {
                      return sys > 0 ? `${detail.face_value}（系统余额 ${sys} + 赠送金余额 ${gift}）` : `${detail.face_value}（赠送金余额）`;
                    }
                    return String(detail.face_value);
                  })()
                  : detail.face_value > 0
                    ? `${detail.remaining} / ${detail.face_value}${Number(detail.worth_value || 0) > detail.face_value ? `（面额价值 ${detail.worth_value}）` : ''}`
                    : '—'}
            </Descriptions.Item>
            {detail.source === 'purchase' && (
              <Descriptions.Item label="卡券模型折扣">
                {detail.credit_discount != null && detail.credit_discount > 0 && detail.credit_discount < 1
                  ? `${Number((detail.credit_discount * 10).toFixed(2))}折 (${detail.credit_discount})`
                  : Object.keys(detail.model_discounts || {}).length > 0
                    ? Object.entries(detail.model_discounts || {})
                      .map(([k, v]) => `${k}: ${Number((Number(v) * 10).toFixed(2))}折`)
                      .join('、')
                    : '原价 (1.0)'}
              </Descriptions.Item>
            )}
            <Descriptions.Item label="兑换码">
              {detail.code ? (
                <Typography.Text copyable={{ text: detail.code }} className="font-mono text-xs">{detail.code}</Typography.Text>
              ) : '—'}
            </Descriptions.Item>
            <Descriptions.Item label="状态">{STATUS_TEXT[detail.status] || detail.status}</Descriptions.Item>
            <Descriptions.Item label="可用模型">
              {detail.source === 'balance'
                ? (Number(detail.system_amount || 0) > 0 && Number(detail.gift_amount || 0) > 0
                  ? '已入系统余额与赠送金余额'
                  : Number(detail.gift_amount || 0) > 0
                    ? '已入赠送金余额'
                    : '已入系统余额')
                : detail.source === 'playground_space'
                  ? '创作中心 2026 存储空间。等级未开放创作中心时不单独开通。'
                  : (detail.models || []).length > 0
                    ? detail.models.join('、')
                    : detail.face_value > 0
                      ? '全部模型'
                      : '—'}
            </Descriptions.Item>
            <Descriptions.Item label="到期">
              {detail.source === 'balance' ? '—' : detail.expires_at ? formatApiDateTime(detail.expires_at) : '长期'}
            </Descriptions.Item>
            <Descriptions.Item label="时间">
              {detail.created_at ? formatApiDateTime(detail.created_at) : '—'}
            </Descriptions.Item>
          </Descriptions>
        ) : null}
      </Modal>
      <Modal
        title="发放卡券"
        open={grantOpen}
        onCancel={() => setGrantOpen(false)}
        onOk={grant}
        confirmLoading={granting}
        okText="发放"
        cancelText="取消"
        destroyOnHidden
      >
        <Select
          style={{ width: '100%' }}
          placeholder="选择卡券会员方案"
          value={planId ?? undefined}
          onChange={(value) => {
            setPlanId(value);
            const next = grantCycles(plans.find((plan) => plan.id === value));
            setGrantCycle(next[0]?.value || '');
          }}
          options={plans.map((plan) => ({ value: plan.id, label: plan.name }))}
        />
        {cycles.length > 0 && (
          <Select
            style={{ width: '100%', marginTop: 8 }}
            value={grantCycle || cycles[0].value}
            onChange={setGrantCycle}
            options={cycles}
          />
        )}
      </Modal>
      <Modal
        title="帮用户兑换"
        open={redeemOpen}
        onCancel={() => setRedeemOpen(false)}
        onOk={redeem}
        confirmLoading={redeeming}
        okText="兑换"
        cancelText="取消"
        destroyOnHidden
      >
        <Input
          value={code}
          placeholder="兑换码"
          onChange={(event) => setCode(event.target.value)}
          onPressEnter={redeem}
        />
      </Modal>
    </div>
  );
}
