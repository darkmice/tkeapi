/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useEffect, useState } from 'react';
import { Card, Form, Input, InputNumber, Button, message, Space, Tabs, Spin, Switch, Radio, Checkbox, Row, Col } from 'antd';
import { SaveOutlined, ArrowLeftOutlined, KeyOutlined, FileTextOutlined, SettingOutlined } from '@ant-design/icons';
import { useTranslation } from 'react-i18next';
import { useParams, useNavigate } from 'react-router-dom';
import { pinyin } from 'pinyin-pro';
import request from '../../utils/request';
import useSettingsStore from '../../store/settings';
import type { UserLevel } from '../../types';

const { TabPane } = Tabs;

const DISCOUNT_OPTIONS = [
  { label: '用户等级折扣', value: 1 },
  { label: '用户模型折扣', value: 2 },
  { label: '模型全站折扣', value: 4 },
  { label: '上游渠道倍率', value: 8 },
];
const ALL_DISCOUNT_VALUES = [1, 2, 4, 8];

const decodeDiscountMask = (dt?: number): number[] => {
  const v = dt ?? 0;
  const mask = v >= 16 ? (v & 15) : (v === 1 ? 6 : v === 2 ? 3 : 15);
  return ALL_DISCOUNT_VALUES.filter(bit => (mask & bit) !== 0);
};

const encodeDiscountMask = (selected?: number[]): number => {
  const mask = (selected || []).reduce((acc, bit) => acc | (bit & 15), 0);
  return 16 | mask;
};

const UserLevelEdit: React.FC = () => {
  const { t } = useTranslation();
  const { actionId } = useParams<{ actionId: string }>();
  const navigate = useNavigate();
  const { settings } = useSettingsStore();
  const adminPath = settings?.site?.admin_path || 'admin1688';
  const currencyUnit = settings?.currency?.currency_unit || '元';
  const [form] = Form.useForm();
  const marketingEnabled = Form.useWatch('marketing_enabled', form);
  const selectedDiscounts: number[] = Form.useWatch('discount_options', form) || [];
  const isAllDiscountsSelected = selectedDiscounts.length === ALL_DISCOUNT_VALUES.length;
  const isIndeterminateDiscounts = selectedDiscounts.length > 0 && selectedDiscounts.length < ALL_DISCOUNT_VALUES.length;
  const isLevelDiscountSelected = selectedDiscounts.includes(1);
  const isMarketingActive = Boolean(marketingEnabled);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [marketingToggleLoading, setMarketingToggleLoading] = useState(false);
  const [groupKeyManuallyEdited, setGroupKeyManuallyEdited] = useState(false);
  const isAdd = actionId === 'new';

  const generateGroupKey = (nameStr: string) => {
    if (!nameStr) return '';
    try {
      const py = pinyin(nameStr, { toneType: 'none', type: 'array' });
      const cleanPy = py.map(p => p.toLowerCase().replace(/[^a-z0-9]/g, '')).join('');
      return cleanPy || 'group_' + Date.now().toString().slice(-4);
    } catch (e) {
      return 'group_' + Date.now().toString().slice(-4);
    }
  };

  const handleNameChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (isAdd && !groupKeyManuallyEdited) {
      const newName = e.target.value;
      const key = generateGroupKey(newName);
      form.setFieldValue('group_key', key);
    }
  };

  useEffect(() => {
    if (isAdd) {
      setLoading(false);
      form.setFieldsValue({
        discount: 1.0,
        discount_options: ALL_DISCOUNT_VALUES,
        commission_ratio: 0.0,
        commission_basis: 'consumption',
        commission_transfer_min: 0,
        commission_transfer_max: 0,
        invite_reward_inviter: 0.0,
        invite_reward_invitee: 0.0,
        daily_invite_limit: 10,
        marketing_enabled: false,
        is_default: false,
        purchasable: false,
        max_token_count: 10,
        allow_view_log_details: true,
        sort_order: 0,
        invoice_enabled: false,
      });
      return;
    }

    const fetchLevel = async () => {
      setLoading(true);
      try {
        const resp = await (request.get('/user_levels') as unknown as Promise<{ data: UserLevel[] }>);
        const level = resp.data.find((l: UserLevel) => String(l.id) === actionId);
        if (level) {
          form.setFieldsValue({
            ...level,
            discount_options: decodeDiscountMask(level.discount_type),
            marketing_enabled: level.marketing_enabled === 1,
            commission_basis: level.commission_basis === 'recharge' ? 'recharge' : 'consumption',
            is_default: level.is_default === 1,
            purchasable: level.purchasable === 1,
            allow_view_log_details: level.allow_view_log_details === undefined ? true : level.allow_view_log_details === 1,
            invoice_enabled: level.invoice_enabled === 1,
          });
        } else {
          message.error('未找到对应等级记录');
          navigate(`/${adminPath}/user-levels`);
        }
      } catch (e) {
        console.error(e);
      } finally {
        setLoading(false);
      }
    };

    fetchLevel();
  }, [actionId, form, isAdd, navigate]);

  const handleSave = async (values: any) => {
    setSaving(true);
    const { discount_options, ...restValues } = values;
    const rawDiscount = values.discount;
    const resolvedDiscount =
      rawDiscount === undefined || rawDiscount === null || Number.isNaN(Number(rawDiscount)) || Number(rawDiscount) <= 0
        ? 1.0
        : Number(rawDiscount);
    // Convert boolean switch back to number, guarding against undefined to prevent overwriting with 0
    const payload = {
      ...restValues,
      discount: resolvedDiscount,
      discount_type: discount_options === undefined ? undefined : encodeDiscountMask(discount_options),
      marketing_enabled: values.marketing_enabled === undefined ? undefined : (values.marketing_enabled ? 1 : 0),
      is_default: values.is_default === undefined ? undefined : (values.is_default ? 1 : 0),
      purchasable: values.purchasable === undefined ? undefined : (values.purchasable ? 1 : 0),
      allow_view_log_details: values.allow_view_log_details === undefined ? undefined : (values.allow_view_log_details ? 1 : 0),
      invoice_enabled: values.invoice_enabled === undefined ? undefined : (values.invoice_enabled ? 1 : 0),
    };
    // 新建时自动生成 group_key
    if (isAdd && !payload.group_key) {
      payload.group_key = generateGroupKey(values.name || '');
    }
    if (!payload.group_key) {
      message.error('分组标识不能为空');
      setSaving(false);
      return;
    }
    try {
      if (isAdd) {
        await request.post('/user_levels', payload);
        message.success(t('user_levels.success'));
      } else {
        await request.put(`/user_levels/${actionId}`, payload);
        message.success(t('user_levels.success'));
      }
      navigate(`/${adminPath}/user-levels`);
    } catch (e) {
      console.error(e);
    } finally {
      setSaving(false);
    }
  };

  /** 专属推广开关：编辑模式下切换即生效，无需点保存配置 */
  const handleMarketingEnabledChange = async (checked: boolean) => {
    form.setFieldValue('marketing_enabled', checked);
    if (isAdd) return;

    setMarketingToggleLoading(true);
    try {
      await request.put(`/user_levels/${actionId}`, {
        marketing_enabled: checked ? 1 : 0,
      });
      message.success(checked ? '专属推广模式已开启' : '专属推广模式已关闭');
    } catch (e) {
      console.error(e);
      form.setFieldValue('marketing_enabled', !checked);
    } finally {
      setMarketingToggleLoading(false);
    }
  };

  if (loading) {
    return <div style={{ textAlign: 'center', marginTop: 100 }}><Spin size="large" /></div>;
  }

  return (
    <Card 
      title={
        <Space>
          <Button icon={<ArrowLeftOutlined />} onClick={() => navigate(`/${adminPath}/user-levels`)} />
          <span>{isAdd ? t('user_levels.add_level') : t('user_levels.edit_level')}</span>
        </Space>
      }
      extra={
        <Button type="primary" icon={<SaveOutlined />} onClick={() => form.submit()} loading={saving}>
          保存配置
        </Button>
      }
      bordered={false}
    >
      <Form form={form} layout="vertical" onFinish={handleSave} style={{ maxWidth: 800 }}>
        <Tabs defaultActiveKey="1" type="card">
          <TabPane tab="等级基本信息" key="1">
            <Form.Item name="name" label={t('user_levels.name')} rules={[{ required: true }]}>
              <Input placeholder="输入等级呈现的中文名称" onChange={handleNameChange} />
            </Form.Item>
            <Form.Item 
              name="group_key" 
              label={t('user_levels.group_key', '等级标志ID (Group Key)')} 
              rules={[{ required: true }]}
              extra="辅助的英文字母等级标识符（如 vip1），创建后固定作为用户等级关联标识。"
            >
              <Input 
                placeholder="e.g. vip, primary" 
                disabled={!isAdd}
                onChange={() => { if(isAdd) setGroupKeyManuallyEdited(true); }}
              />
            </Form.Item>
            <Form.Item
              name="discount_options"
              label={
                <Space size={12}>
                  <span>折扣使用设置</span>
                  <Checkbox
                    indeterminate={isIndeterminateDiscounts}
                    checked={isAllDiscountsSelected}
                    onChange={(e) => {
                      const next = e.target.checked ? ALL_DISCOUNT_VALUES : [];
                      form.setFieldValue('discount_options', next);
                      if (!next.includes(1)) {
                        const cur = form.getFieldValue('discount');
                        if (cur === undefined || cur === null || Number(cur) <= 0) {
                          form.setFieldValue('discount', 1.0);
                        }
                        form.validateFields(['discount']).catch(() => {});
                      }
                    }}
                  >
                    全选
                  </Checkbox>
                </Space>
              }
              extra={
                selectedDiscounts.length === 0 ? (
                  <span style={{ color: 'var(--ant-color-warning)' }}>
                    当前未勾选任何折扣项：该等级调用所有模型均不享受优惠，统一按 1.00x 原价计费（若模型限价高于 1.00x 则按模型限价兜底）。
                  </span>
                ) : (
                  `已勾选 ${selectedDiscounts.length} 项：系统自动匹配勾选项中的最低值（最大优惠）使用；若低于模型折扣限价，则直接使用模型限价兜底，保证平台不亏损。`
                )
              }
            >
              <Checkbox.Group
                options={DISCOUNT_OPTIONS}
                onChange={(vals) => {
                  if (!vals.includes(1)) {
                    const cur = form.getFieldValue('discount');
                    if (cur === undefined || cur === null || Number(cur) <= 0) {
                      form.setFieldValue('discount', 1.0);
                    }
                    form.validateFields(['discount']).catch(() => {});
                  }
                }}
              />
            </Form.Item>
            <Form.Item 
              name="discount" 
              label={t('user_levels.discount', '计费倍率 (Discount/Multiplier)')} 
              rules={[
                { required: isLevelDiscountSelected, message: '请输入计费倍率' },
                {
                  validator: (_, val) => {
                    if (!isLevelDiscountSelected) return Promise.resolve();
                    if (val === undefined || val === null || Number.isNaN(Number(val)) || Number(val) <= 0) {
                      return Promise.reject(new Error('计费倍率必须大于 0'));
                    }
                    return Promise.resolve();
                  },
                },
              ]}
              extra={
                isLevelDiscountSelected ? (
                  '设置用户计费的乘数。1 为原价不打折；小于1为折扣优惠（如 0.8 为 8折）；大于1为加倍收费（如 1.5 表明加价50%）。'
                ) : (
                  <span style={{ color: 'var(--ant-color-warning)' }}>
                    上方未勾选「用户等级折扣」，此等级倍率暂不参与计费（如需启用请在上方勾选「用户等级折扣」）。
                  </span>
                )
              }
            >
              <InputNumber disabled={!isLevelDiscountSelected} style={{ width: '100%' }} min={0.01} max={999} step={0.01} precision={2} />
            </Form.Item>
            <Form.Item name="description" label={t('user_levels.description')}>
              <Input.TextArea rows={4} placeholder="描述该组特权或补充信息..." />
            </Form.Item>
            <Form.Item 
              name="sort_order" 
              label="排序（数字越大越靠前）" 
              extra="数字大的将显示在列表前面"
            >
              <InputNumber style={{ width: '100%' }} />
            </Form.Item>
            <Form.Item 
              name="is_default" 
              label="设为默认注册等级" 
              valuePropName="checked"
              extra="开启后，新用户注册时将自动成为该等级。同一时间只能有一个默认注册等级，设置后会覆盖之前的默认等级。"
            >
              <Switch />
            </Form.Item>
            <Form.Item
              name="purchasable"
              label="可购买用户等级"
              valuePropName="checked"
              extra="开启后，这个等级才会出现在「营销卡券会员 → 会员卡券方案」的会员等级里，可以被选为升级目标。关闭后，新建方案选不到它。"
            >
              <Switch />
            </Form.Item>
          </TabPane>

          <TabPane tab="等级营销推广" key="2" forceRender>
            <div className="space-y-4 pt-1">
              {/* 专属推广模式开关卡片 */}
              <div className="rounded-xl border border-border bg-card p-5 shadow-sm">
                <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                  <div className="space-y-1">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-semibold text-foreground">专属推广模式</span>
                      <span className={`text-[11px] px-2 py-0.5 rounded-full border font-medium transition-colors ${
                        isMarketingActive 
                          ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400' 
                          : 'border-border bg-muted text-muted-foreground'
                      }`}>
                        {isMarketingActive ? '已开启' : '已关闭'}
                      </span>
                      <span className="text-[11px] px-2 py-0.5 rounded-full border border-border bg-muted/60 text-muted-foreground font-medium">高优先级</span>
                      {!isAdd && (
                        <span className="text-[11px] text-muted-foreground/70 hidden sm:inline">（切换即时生效）</span>
                      )}
                    </div>
                    <p className="text-xs text-muted-foreground m-0">
                      开启后按下方比例发放新返佣，并用本等级注册赠送覆盖全局赠礼。关闭后不再产生新返佣，已入账佣金保留；注册赠送回到全局配置。
                    </p>
                  </div>
                  <div className="shrink-0 flex items-center">
                    <Form.Item name="marketing_enabled" valuePropName="checked" className="mb-0">
                      <Switch
                        loading={marketingToggleLoading}
                        onChange={handleMarketingEnabledChange}
                      />
                    </Form.Item>
                  </div>
                </div>
              </div>

              {/* 受开关控制的配置卡片区域 */}
              <div className={`space-y-4 transition-all duration-200 ${!isMarketingActive ? 'opacity-60 select-none' : ''}`}>
                {/* 返佣规则卡片 */}
                <div className="rounded-xl border border-border bg-card shadow-sm">
                  <div className="px-5 py-3.5 border-b border-border/60">
                    <div className="text-sm font-semibold text-foreground">返佣规则</div>
                    <div className="text-xs text-muted-foreground mt-0.5">配置邀请人获得的返利比例与结算基准</div>
                  </div>
                  <div className="p-5">
                    <Row gutter={[16, 16]}>
                      <Col xs={24} sm={12}>
                        <Form.Item 
                          name="commission_ratio" 
                          label="返利比例 (邀请返现)" 
                          rules={isMarketingActive ? [{ required: true, message: '请输入返利比例' }] : []}
                          className="mb-0"
                        >
                          <InputNumber 
                            disabled={!isMarketingActive}
                            style={{ width: '100%' }} 
                            min={0} 
                            max={1} 
                            step={0.01} 
                            precision={2} 
                            placeholder="0% ~ 100%"
                            formatter={value => `${Math.round((Number(value) || 0) * 100)}%`}
                            parser={value => (parseFloat(value?.replace('%', '') || '0') / 100) as any}
                          />
                        </Form.Item>
                      </Col>
                      <Col xs={24} sm={12}>
                        <Form.Item
                          name="commission_basis"
                          label="返佣发放基准"
                          rules={isMarketingActive ? [{ required: true, message: '请选择发放基准' }] : []}
                          className="mb-0"
                        >
                          <Radio.Group 
                            disabled={!isMarketingActive}
                            optionType="button" 
                            buttonStyle="solid" 
                            className="w-full flex"
                          >
                            <Radio.Button value="consumption" className="flex-1 text-center">按实际消费</Radio.Button>
                            <Radio.Button value="recharge" className="flex-1 text-center">按充值发放</Radio.Button>
                          </Radio.Group>
                        </Form.Item>
                      </Col>
                    </Row>
                    <div className="mt-3 text-[12px] text-muted-foreground/80 flex items-center gap-1.5">
                      <span>💡</span>
                      <span>实际消费按调用净额计入，充值按在线支付到账即时发放。返利直接进入邀请人佣金钱包。</span>
                    </div>
                  </div>
                </div>

                {/* 佣金划转限额卡片 */}
                <div className="rounded-xl border border-border bg-card shadow-sm">
                  <div className="px-5 py-3.5 border-b border-border/60">
                    <div className="text-sm font-semibold text-foreground">佣金划转限额</div>
                    <div className="text-xs text-muted-foreground mt-0.5">控制佣金钱包划转至系统可用余额的额度门槛</div>
                  </div>
                  <div className="p-5">
                    <Row gutter={[16, 16]}>
                      <Col xs={24} sm={12}>
                        <Form.Item
                          name="commission_transfer_min"
                          label="最低划转金额"
                          rules={isMarketingActive ? [{ required: true, message: '请输入最低划转金额' }] : []}
                          className="mb-0"
                        >
                          <InputNumber 
                            disabled={!isMarketingActive}
                            style={{ width: '100%' }} 
                            min={0} 
                            step={1} 
                            precision={2} 
                            placeholder="0 为不限制"
                            addonAfter={currencyUnit}
                          />
                        </Form.Item>
                      </Col>
                      <Col xs={24} sm={12}>
                        <Form.Item
                          name="commission_transfer_max"
                          label="单次划转上限"
                          rules={isMarketingActive ? [{ required: true, message: '请输入单次最多划转金额' }] : []}
                          className="mb-0"
                        >
                          <InputNumber 
                            disabled={!isMarketingActive}
                            style={{ width: '100%' }} 
                            min={0} 
                            step={1} 
                            precision={2} 
                            placeholder="0 为不限制"
                            addonAfter={currencyUnit}
                          />
                        </Form.Item>
                      </Col>
                    </Row>
                  </div>
                </div>

                {/* 注册赠礼与每日限额卡片 */}
                <div className="rounded-xl border border-border bg-card shadow-sm">
                  <div className="px-5 py-3.5 border-b border-border/60">
                    <div className="text-sm font-semibold text-foreground">注册赠礼与每日限额</div>
                    <div className="text-xs text-muted-foreground mt-0.5">新用户通过邀请注册时的双向额度奖励及每日防刷上限</div>
                  </div>
                  <div className="p-5">
                    <Row gutter={[16, 16]}>
                      <Col xs={24} sm={8}>
                        <Form.Item 
                          name="invite_reward_inviter" 
                          label="邀请人奖励额度" 
                          rules={isMarketingActive ? [{ required: true, message: '请输入邀请人奖励' }] : []}
                          className="mb-0"
                        >
                          <InputNumber 
                            disabled={!isMarketingActive}
                            style={{ width: '100%' }} 
                            min={0} 
                            step={1} 
                            precision={2} 
                            placeholder="0.00"
                            addonAfter={currencyUnit}
                          />
                        </Form.Item>
                      </Col>
                      <Col xs={24} sm={8}>
                        <Form.Item 
                          name="invite_reward_invitee" 
                          label="受邀人新客额度" 
                          rules={isMarketingActive ? [{ required: true, message: '请输入受邀人奖励' }] : []}
                          className="mb-0"
                        >
                          <InputNumber 
                            disabled={!isMarketingActive}
                            style={{ width: '100%' }} 
                            min={0} 
                            step={1} 
                            precision={2} 
                            placeholder="0.00"
                            addonAfter={currencyUnit}
                          />
                        </Form.Item>
                      </Col>
                      <Col xs={24} sm={8}>
                        <Form.Item 
                          name="daily_invite_limit" 
                          label="每日奖励名额上限" 
                          rules={isMarketingActive ? [{ required: true, message: '请输入每日上限' }] : []}
                          className="mb-0"
                        >
                          <InputNumber 
                            disabled={!isMarketingActive}
                            style={{ width: '100%' }} 
                            min={0} 
                            step={1} 
                            precision={0} 
                            placeholder="0 为无限制"
                            addonAfter="人/天"
                          />
                        </Form.Item>
                      </Col>
                    </Row>
                  </div>
                </div>
              </div>
            </div>
          </TabPane>

          <TabPane tab={<span><KeyOutlined /> 密钥配置</span>} key="3" forceRender>
            <Form.Item 
              name="max_token_count" 
              label="最大密钥创建数量" 
              rules={[{ required: true }]}
              extra="限制该等级用户可以创建的 API 密钥数量上限。设为 0 表示禁止创建密钥。"
            >
              <InputNumber style={{ width: '100%' }} min={0} max={1000} step={1} precision={0} />
            </Form.Item>
          </TabPane>

          <TabPane tab="日志配置" key="4" forceRender>
            <Form.Item 
              name="allow_view_log_details" 
              label="查看日志详情" 
              valuePropName="checked"
              extra="设置关闭后，属于当前用户等级的用户在页面的使用日志和任务日志里只能看列表信息，无法点击下拉看详细内容（同时隐藏使用日志的详情文案和任务日志的加号图标），设置开启后才能看。"
            >
              <Switch />
            </Form.Item>
          </TabPane>

          <TabPane tab={<span><FileTextOutlined /> Invoices收据&发票</span>} key="5" forceRender>
            <Form.Item 
              name="invoice_enabled" 
              label="开启该用户等级的 Invoices 收据与发票功能" 
              valuePropName="checked"
              extra="开启后，属于该等级的用户登录时可在「用户设置 -> 财务发票 (Invoices)」中使用发票与交易凭单功能；关闭则对该等级用户隐藏。"
            >
              <Switch />
            </Form.Item>

            <Card 
              size="small" 
              style={{ 
                marginTop: 16, 
                borderRadius: 8, 
                background: 'var(--ant-color-bg-container)',
                border: '1px solid var(--ant-color-border-secondary)'
              }}
            >
              <Space direction="vertical" size={12} style={{ width: '100%' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 14, fontWeight: 500 }}>
                  <FileTextOutlined style={{ color: '#722ed1', fontSize: 18 }} />
                  <span>发票与收据全局业务参数统一配置</span>
                </div>
                <div style={{ color: 'var(--ant-color-text-secondary)', fontSize: 13, lineHeight: 1.6 }}>
                  发票开票主体、纳税人识别号、销方联系邮箱、国际海外模式（商业发票/收据前缀、货币符号、打印条款）与中国大陆境内增值税发票规则（起开门槛、税率、应税劳务类目）等全局业务参数，已统一移至<b>「支付财务设置 → Invoices收据&amp;发票」</b>中集中维护。在此处开启本等级权限后，该等级用户即可使用全局配置的发票功能。
                </div>
                <div style={{ marginTop: 4 }}>
                  <Button 
                    type="primary" 
                    icon={<SettingOutlined />}
                    onClick={() => navigate(`/${adminPath}/payment-settings?tab=invoices`)}
                    style={{
                      backgroundColor: '#722ed1',
                      borderColor: '#722ed1',
                      color: '#ffffff',
                      fontWeight: 500,
                      borderRadius: 6,
                      boxShadow: '0 2px 8px rgba(114, 46, 209, 0.4)',
                    }}
                  >
                    前往「支付财务设置」配置发票详细规则
                  </Button>
                </div>
              </Space>
            </Card>
          </TabPane>
        </Tabs>
      </Form>
    </Card>
  );
};

export default UserLevelEdit;
