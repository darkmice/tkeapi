/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import i18n from '../i18n';

/** 历史脏值 → 规范 type */
const ALIAS: Record<string, string> = {
  'allinpay wechat': 'allinpay_wechat',
  'allinpay alipay': 'allinpay_alipay',
};

const BRAND: Record<string, string> = {
  stripe: 'Stripe',
  hyperbc: 'HyperBC',
  bonuspay: 'BonusPay',
};

const COLOR: Record<string, string> = {
  manual: 'blue',
  gift: 'cyan',
  registration: 'magenta',
  commission: 'gold',
  alipay: 'blue',
  wechat: 'green',
  allinpay_wechat: 'green',
  allinpay_alipay: 'blue',
  stripe: 'purple',
  bonuspay: 'geekblue',
  hyperbc: 'purple',
  redemption: 'orange',
  redemption_refund: 'orange',
  model_voucher: 'gold',
  coupon_purchase: 'purple',
  coupon_pack: 'purple',
  coupon_subscription: 'blue',
  ark_video_consume: 'volcano',
  ark_video_refund: 'green',
};

const WALLET_RECHARGE_FILTERS = [
  'manual',
  'gift',
  'registration',
  'alipay',
  'wechat',
  'redemption',
  'model_voucher',
  'coupon_purchase',
  'coupon_pack',
  'coupon_subscription',
] as const;

function norm(type: string): string {
  return ALIAS[type] || type;
}

/** 充值类型文案（固定读 translation/finance.*，不依赖调用方 useTranslation 的 ns） */
export function rechargeTypeLabel(type: string, walletType?: string | null): string {
  if (!type) return '-';
  if (type === 'manual' && walletType === 'credit') {
    return i18n.t('finance.recharge_type_manual_credit');
  }
  const key = norm(type);
  if (BRAND[key]) return BRAND[key];
  const i18nKey = `finance.recharge_type_${key}`;
  const label = i18n.t(i18nKey);
  return !label || label === i18nKey ? type : label;
}

export function rechargeTypeColor(type: string): string {
  return COLOR[norm(type)] || 'default';
}

export function rechargeTypeFilters() {
  return WALLET_RECHARGE_FILTERS.map((value) => ({
    text: rechargeTypeLabel(value),
    value,
  }));
}

/**
 * 提取订单号：优先读取实体的 order_no 字段，若无则从 remark 中提取
 * 例如:
 *  - "微信支付充值 订单号:T20260907055127R9a047a0f" -> "T20260907055127R9a047a0f"
 *  - "支付宝充值 订单号：202609070551" -> "202609070551"
 *  - "T20260907055127R9a047a0f" -> "T20260907055127R9a047a0f"
 *  - 若无订单号返回 null
 */
export function extractOrderNo(
  input?: { order_no?: string | null; remark?: string | null } | string | null
): string | null {
  if (!input) return null;
  if (typeof input === 'object') {
    if (input.order_no && typeof input.order_no === 'string' && input.order_no.trim()) {
      return input.order_no.trim();
    }
    return extractOrderNo(input.remark);
  }
  const trimmed = input.trim();
  if (!trimmed) return null;

  // 1. 匹配带 "订单号:" 或 "Order No:" 等前缀的单号
  const match = trimmed.match(/(?:订单号|Order\s*(?:No|Number|Id)|单号)[\s:：]+([a-zA-Z0-9_-]+)/i);
  if (match && match[1]) {
    return match[1];
  }

  // 2. 匹配纯标准订单号 (如系统充值 T20260907... 或 BonusPay 的 BP... 或 赠送金 G20260907...)
  if (/^[TG]\d{14}[A-Za-z0-9_-]+$/.test(trimmed) || /^BP[A-Za-z0-9_-]+$/.test(trimmed)) {
    return trimmed;
  }

  return null;
}

/**
 * 精简完善备注：去除备注中已被单独提取出订单号的冗余说明
 * 例如:
 *  - "微信支付充值 订单号:T20260907055127R9a047a0f" -> "" (订单号与类型已在单独列展示，备注清空为 -)
 *  - "支付宝充值 订单号:202609070551" -> ""
 *  - "T20260907055127R9a047a0f" -> ""
 *  - "BonusPay 充值 订单号:BP123 10 USDT (txHash: 0xabc)" -> "10 USDT (txHash: 0xabc)"
 *  - "初始龙套户 公司支付宝已收款5000元 充值可提" -> "初始龙套户 公司支付宝已收款5000元 充值可提" (业务备注完整保留)
 */
export function cleanRemark(remark?: string | null, _orderNo?: string | null): string {
  if (!remark || typeof remark !== 'string') return '';
  const text = remark.trim();
  if (!text) return '';

  // 1. 若纯粹是系统自动生成的充值/赠送单号说明（单号已提至订单号列，类型已有标签，无其它附言时直接清空）
  if (
    /^([^\s]+充值\s*)?(?:订单号|Order\s*(?:No|Id)|单号)[\s:：]*[a-zA-Z0-9_-]+$/i.test(text) ||
    /^[TG]\d{14}[A-Za-z0-9_-]+$/.test(text) ||
    /^BP[A-Za-z0-9_-]+$/.test(text)
  ) {
    return '';
  }

  // 2. 若备注中包含单号且还有其他附言信息，仅剔除冗余的单号前缀部分，保留真实业务附言
  const cleaned = text
    .replace(/(?:[^\s]+充值\s*)?(?:订单号|Order\s*(?:No|Id)|单号)[\s:：]*[a-zA-Z0-9_-]+/gi, '')
    .replace(/^[\s,，:：;；()（）-]+|[\s,，:：;；()（）-]+$/g, '')
    .trim();

  return cleaned;
}
