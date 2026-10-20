/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Alert,
  Button,
  Input,
  Modal,
  Popover,
  Typography,
  message,
} from 'antd';
import {
  GiftOutlined,
  WalletOutlined,
  AppstoreOutlined,
  CheckOutlined,
  CopyOutlined,
  InfoCircleOutlined,
  CrownOutlined,
  ThunderboltOutlined,
  HistoryOutlined,
  CloudOutlined,
  TagOutlined,
} from '@ant-design/icons';
import { useNavigate, useLocation } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import request from '../../utils/request';
import useAuthStore from '../../store/auth';
import useSettingsStore from '../../store/settings';
import { fetchActivePlugins } from '../../utils/activePlugins';
import { formatApiDateTime } from '../../utils/timedisplay';
import { formatCreditMoney } from './money';
import { couponCampaignPath, offerReturnState } from './storefront/path';
import PageHeader from '../../components/PageHeader';

type Credit = {
  id: number;
  plan_name: string;
  face_value: number;
  worth_value?: number;
  credit_discount?: number;
  model_discounts?: Record<string, number>;
  remaining: number;
  model_ids: string[];
  expires_at: string | null;
  status: string;
  created_at: string;
  updated_at?: string | null;
  offer_kind?: string;
  plan_id?: number;
  billing_cycle?: string;
};

type SpaceGrant = {
  id: number;
  plan_name: string;
  space_mb: number;
  expires_at: string | null;
  status: string;
  created_at: string;
  updated_at?: string | null;
  offer_kind?: string;
  plan_id?: number;
  billing_cycle?: string;
  order_no?: string;
};

type MemberSub = {
  plan_id: number;
  name: string;
  status: string;
  expires_at: string | null;
  billing_cycle: string;
  renew_cycle: string;
  renew_state: string;
  content_changed?: boolean;
  credit_remaining: number;
  credit_face: number;
  credit_worth?: number;
  credit_discount?: number;
  model_discounts?: Record<string, number>;
  models: string[];
  space_mb: number;
  next_space_mb?: number | null;
  next_space_at?: string | null;
  next_credit_face?: number | null;
  next_credit_at?: string | null;
};

type Voucher = {
  id: number;
  name: string;
  description?: string;
  face_value: number;
  worth_value?: number;
  credit_discount?: number;
  model_discounts?: Record<string, number>;
  system_amount?: number;
  gift_amount?: number;
  remaining: number;
  allowed_models: string[];
  expires_at: string | null;
  redeemed_at: string;
  status: string;
  code?: string;
  source?: 'redeem' | 'balance' | 'playground_space';
  space_mb?: number;
  updated_at?: string | null;
};

type CardRow = {
  key: string;
  source: 'subscription' | 'pack' | 'redeem' | 'balance' | 'playground_space';
  card_type?: 'credit' | 'space';
  name: string;
  description?: string;
  face_value: number;
  worth_value?: number;
  credit_discount?: number;
  model_discounts?: Record<string, number>;
  system_amount?: number;
  gift_amount?: number;
  remaining: number;
  models: string[];
  expires_at: string | null;
  status: string;
  created_at: string;
  updated_at?: string | null;
  serial?: string;
  plan_id?: number;
  billing_cycle?: string;
  space_mb?: number;
};

const THREE_DAYS_MS = 3 * 24 * 60 * 60 * 1000;

function isCardActive(card: CardRow, nowMs: number = Date.now()): boolean {
  if (card.status !== 'active') return false;
  if (card.expires_at) {
    const exp = new Date(card.expires_at).getTime();
    if (!isNaN(exp) && exp <= nowMs) {
      return false;
    }
  }
  return true;
}

function isCardRecentExpiredOrExhausted(card: CardRow, nowMs: number = Date.now()): boolean {
  // 1. 已失效判定（status 为 expired/voided/returned，或 status 为 active 但实际 expires_at 已超时）
  const isExpired =
    card.status === 'expired' ||
    card.status === 'voided' ||
    card.status === 'returned' ||
    (card.status === 'active' && !!card.expires_at && new Date(card.expires_at).getTime() <= nowMs);

  if (isExpired) {
    if (card.expires_at) {
      const exp = new Date(card.expires_at).getTime();
      if (!isNaN(exp) && exp <= nowMs && nowMs - exp <= THREE_DAYS_MS) {
        return true;
      }
    }
    const rec = card.updated_at ? new Date(card.updated_at).getTime() : new Date(card.created_at).getTime();
    if (!isNaN(rec) && nowMs - rec >= 0 && nowMs - rec <= THREE_DAYS_MS) {
      return true;
    }
  }

  // 2. 已耗尽判定（status 为 exhausted 或 credited）
  const isExhausted = card.status === 'exhausted' || card.status === 'credited';
  if (isExhausted) {
    const rec = card.updated_at ? new Date(card.updated_at).getTime() : new Date(card.created_at).getTime();
    if (!isNaN(rec) && nowMs - rec >= 0 && nowMs - rec <= THREE_DAYS_MS) {
      return true;
    }
    if (card.expires_at) {
      const exp = new Date(card.expires_at).getTime();
      if (!isNaN(exp) && exp <= nowMs && nowMs - exp <= THREE_DAYS_MS) {
        return true;
      }
    }
  }

  return false;
}

function membershipVisible(plugin: any, user: { role?: string; user_group?: string; level_id?: number | string | null } | null) {
  if (!plugin) return false;
  if (plugin.allowed_levels === 'all' || user?.role === 'admin') return true;
  const allowed = String(plugin.allowed_levels || '').split(',');
  const userGroup = user?.user_group || '';
  const levelId = user?.level_id != null ? String(user.level_id) : '';
  return allowed.includes(userGroup) || (levelId !== '' && allowed.includes(levelId));
}

/**
 * 暗黑科技风卡券卡片组件 (跟随顶部区域深色设计语言)
 */
const CouponCard: React.FC<{
  card: CardRow;
  symbol: string;
  en: boolean;
  onNavigateWallet: () => void;
}> = ({ card, symbol, en, onNavigateWallet }) => {
  const isBalance = card.source === 'balance';
  const isSubscription = card.source === 'subscription';
  const isPack = card.source === 'pack';
  const isSpace = card.card_type === 'space';

  // 状态判定：active(可用/生效中)，credited(已入账至钱包，视为已使用/已失效)
  const isCredited = card.status === 'credited';
  const isExpiredByTime = !!card.expires_at && new Date(card.expires_at).getTime() <= Date.now();
  const isActive = card.status === 'active' && !isExpiredByTime;
  const isVoidedOrReturned = card.status === 'voided' || card.status === 'returned';
  const isRecentlyInactive = !isActive && isCardRecentExpiredOrExhausted(card);

  // 剩余百分比（空血条判定：余额券已全额划转进钱包、退还作废、已耗尽、已过期等均为空血条 0%）
  const percentRemaining = (() => {
    if (isBalance || isVoidedOrReturned || card.status === 'exhausted' || card.status === 'expired' || !isActive) {
      return 0;
    }
    if (card.face_value > 0) {
      return Math.min(100, Math.max(0, Math.round((card.remaining / card.face_value) * 100)));
    }
    return 0;
  })();

  const isExpiringSoon = (() => {
    if (!card.expires_at || !isActive) return false;
    const diff = new Date(card.expires_at).getTime() - Date.now();
    return diff > 0 && diff <= 3 * 24 * 3600 * 1000;
  })();

  const sourceMeta = (() => {
    if (isSpace) {
      return card.source === 'playground_space'
        ? { label: en ? 'CREATIVE SPACE' : '创作空间', icon: <CloudOutlined className="text-xs text-zinc-400" /> }
        : { label: en ? 'STORAGE PACK' : '空间加油包', icon: <CloudOutlined className="text-xs text-zinc-400" /> };
    }
    if (isBalance) return { label: en ? 'BALANCE VOUCHER' : '余额卡券', icon: <WalletOutlined className="text-xs text-zinc-400" /> };
    if (isSubscription) return { label: en ? 'MEMBERSHIP SUBSCRIPTION' : '会员订阅', icon: <CrownOutlined className="text-xs text-zinc-400" /> };
    if (isPack) return { label: en ? 'FUEL PACK' : '加油包', icon: <ThunderboltOutlined className="text-xs text-zinc-400" /> };
    return { label: en ? 'MODEL VOUCHER' : '模型券', icon: <GiftOutlined className="text-xs text-zinc-400" /> };
  })();

  return (
    <div
      className={`rounded-xl border py-2 px-3.5 sm:py-2 sm:px-4 transition-all flex flex-col lg:flex-row lg:items-center justify-between gap-2 select-none ${
        isActive
          ? 'border-zinc-200 dark:border-zinc-800 bg-[#fafafa] dark:bg-[#1c1c1f] hover:border-zinc-300 dark:hover:border-zinc-700 shadow-xs'
          : 'border-zinc-200/60 dark:border-zinc-800/40 bg-zinc-100/50 dark:bg-[#141414]/60 opacity-60'
      }`}
    >
      {/* 1. 左侧主要信息区：来源药丸、名称、状态胶囊、卡号/订单号、模型/空间范围、有效期限 */}
      <div className="space-y-0.5 flex-1 min-w-0">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="flex items-center gap-1.5 text-[11px] font-medium text-zinc-500 dark:text-zinc-400">
            {sourceMeta.icon}
            <span>
              {sourceMeta.label}
            </span>
          </span>

          <span className="text-sm font-semibold text-zinc-900 dark:text-white tracking-tight truncate">
            {card.name}
          </span>

          {isCredited ? (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-zinc-100 dark:bg-zinc-800 text-zinc-500 dark:text-zinc-400 border border-zinc-200 dark:border-zinc-700/60">
              <span className="w-1.5 h-1.5 rounded-full bg-zinc-400" />
              {en ? 'CREDITED' : '已入账'}
            </span>
          ) : isActive ? (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-zinc-100 dark:bg-zinc-800 text-zinc-800 dark:text-zinc-200 border border-zinc-200 dark:border-zinc-700/60">
              <span className="w-1.5 h-1.5 rounded-full bg-zinc-500 dark:bg-zinc-400" />
              {en ? 'ACTIVE' : '生效中'}
            </span>
          ) : (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-zinc-800/40 text-zinc-500 border border-zinc-800">
              <span className="w-1.5 h-1.5 rounded-full bg-zinc-600" />
              {card.status === 'exhausted'
                ? (en ? 'EXHAUSTED' : '已耗尽')
                : card.status === 'returned'
                ? (en ? 'RETURNED' : '已退还')
                : isVoidedOrReturned
                ? (card.source === 'playground_space'
                  ? (en ? 'REVOKED' : '已收回')
                  : (en ? 'VOIDED' : '已作废'))
                : (en ? 'EXPIRED' : '已失效')}
            </span>
          )}

          {isRecentlyInactive && (
            <span className="inline-flex items-center px-1.5 py-0.5 rounded-md text-[10px] font-medium bg-zinc-800 text-amber-400/90 border border-amber-500/20">
              {en ? 'Past 3 days' : '近3天'}
            </span>
          )}

          {isExpiringSoon && (
            <span className="inline-flex items-center px-2 py-0.5 rounded-md text-[10px] font-medium bg-zinc-800 text-amber-400/90 border border-amber-500/20">
              {en ? 'Expiring soon' : '即将到期'}
            </span>
          )}
        </div>

        {card.description && card.description.trim() && (
          <div className="text-[11px] text-zinc-500 dark:text-zinc-400 leading-relaxed break-words">
            {card.description.trim()}
          </div>
        )}

        {/* 辅助属性行 1：卡号（普通卡券展示卡号，空间加油包不展示订单号）、适用范围/模型 */}
        <div className="flex flex-wrap items-center gap-x-3.5 gap-y-0.5 text-[11px] text-zinc-500 dark:text-zinc-400">
          {(!isSpace || card.source === 'playground_space') && card.serial && (
            <div className="flex items-center gap-1">
              <span>{en ? 'Code:' : '卡号:'}</span>
              <Typography.Text
                copyable={{
                  text: card.serial,
                  icon: [
                    <CopyOutlined key="copy" className="text-[11px] text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-200" />,
                    <CheckOutlined key="copied" className="text-[11px] text-emerald-500 dark:text-emerald-400" />,
                  ],
                }}
                style={{ color: 'inherit' }}
                className="text-[11px] tracking-wider text-zinc-500 dark:text-zinc-400"
              >
                {card.serial.length > 16 ? `${card.serial.slice(0, 6)}••••${card.serial.slice(-4)}` : card.serial}
              </Typography.Text>
            </div>
          )}

          {isSpace ? (
            <div className="flex items-center gap-1">
              <span>{en ? 'Scope:' : '适用范围:'}</span>
              <span>{en ? 'Creative Center Storage' : '创作中心存储空间'}</span>
            </div>
          ) : card.models && card.models.length > 0 ? (
            <div className="flex items-center gap-1">
              <span>{en ? 'Models:' : '适用范围:'}</span>
              <Popover
                title={<span className="text-zinc-900 dark:text-white font-semibold">{en ? 'Applicable Models' : '适用模型范围'}</span>}
                content={
                  <div className="max-w-xs max-h-48 overflow-y-auto flex flex-wrap gap-1 p-1">
                    {card.models.map((m) => (
                      <span key={m} className="px-2 py-0.5 rounded text-[11px] bg-zinc-100 dark:bg-zinc-800 text-zinc-800 dark:text-zinc-200 border border-zinc-200 dark:border-white/10">{m}</span>
                    ))}
                  </div>
                }
              >
                <span className="cursor-pointer underline decoration-dotted underline-offset-2 hover:text-zinc-700 dark:hover:text-zinc-200">
                  {card.models.length <= 2 ? card.models.join(', ') : `${card.models[0]} +${card.models.length - 1}`}
                </span>
              </Popover>
            </div>
          ) : (
            <div className="flex items-center gap-1">
              <span>{en ? 'Models:' : '适用范围:'}</span>
              <span>{en ? 'All Models' : '全平台通用'}</span>
            </div>
          )}

          {!isSpace && !isBalance && ((card.credit_discount != null && card.credit_discount > 0 && card.credit_discount < 1) || Object.keys(card.model_discounts || {}).length > 0) && (
            <div className="flex items-center gap-1">
              <span>{en ? 'Discount:' : '卡券折扣:'}</span>
              <span className="font-medium text-zinc-700 dark:text-zinc-200">
                {card.credit_discount != null && card.credit_discount > 0 && card.credit_discount < 1
                  ? (en ? `${Math.round(card.credit_discount * 100)}%` : `${Number((card.credit_discount * 10).toFixed(2))}折`)
                  : (en ? 'Model Specific' : '模型专属折扣')}
              </span>
            </div>
          )}
        </div>

        {/* 辅助属性行 2：时间信息（独立换行：发放时间排前面，有效期排后面，时间文字大小规格完全一致） */}
        <div className="flex flex-wrap items-center gap-x-3.5 gap-y-0.5 text-[11px] text-zinc-500 dark:text-zinc-400">
          <div className="flex items-center gap-1">
            <span>{en ? 'Issued:' : '发放时间:'}</span>
            <span className="font-normal">{formatApiDateTime(card.created_at, 'YYYY-MM-DD')}</span>
          </div>

          <div className="flex items-center gap-1">
            <span>{isBalance ? (en ? 'Redeemed:' : '入账时间:') : (en ? 'Expires:' : '有效期至:')}</span>
            <span className={`font-normal ${isExpiringSoon ? 'text-amber-500 dark:text-amber-400/90' : ''}`}>
              {card.expires_at
                ? formatApiDateTime(card.expires_at, 'YYYY-MM-DD HH:mm')
                : isBalance
                ? formatApiDateTime(card.created_at, 'YYYY-MM-DD HH:mm')
                : (en ? 'Permanent' : '长期有效')}
            </span>
          </div>
        </div>
      </div>

      {/* 2. 右侧额度/空间数据区与进度条 */}
      <div className="shrink-0 flex flex-col justify-center min-w-[200px] sm:min-w-[260px] pt-1.5 lg:pt-0 border-t lg:border-t-0 border-zinc-200 dark:border-zinc-800">
        <div className="flex items-baseline justify-between gap-3 mb-1">
          {/* 面值 / 扩容量 */}
          <div className="flex items-baseline gap-1 flex-wrap">
            <span className="text-xs text-zinc-500 font-medium">
              {isSpace ? (en ? 'Storage' : '扩容容量') : (en ? 'Value' : '面值')}
            </span>
            <span className={`text-xs font-medium ${!isActive ? 'text-zinc-500' : 'text-zinc-700 dark:text-zinc-200'}`}>
              {isSpace ? `+${card.space_mb || 0} MB` : formatCreditMoney(symbol, card.face_value)}
            </span>
            {!isSpace && (card.worth_value || 0) > card.face_value && (
              <span className="text-[11px] text-zinc-400">
                {en ? `(Worth ${formatCreditMoney(symbol, card.worth_value || 0)})` : `(价值 ${formatCreditMoney(symbol, card.worth_value || 0)})`}
              </span>
            )}
          </div>

          <div className="text-right text-xs text-zinc-500 dark:text-zinc-400">
            {isVoidedOrReturned ? (
              <span className="text-zinc-500 font-medium text-xs">
                {card.status === 'returned' ? (en ? 'Returned' : '已退还') : (en ? 'Voided' : '已作废')}
              </span>
            ) : isSpace ? (
              !isActive && (
                <span className="text-zinc-500 font-medium text-xs">
                  {card.status === 'expired' ? (en ? 'Expired' : '已到期') : (en ? 'Inactive' : '已失效')}
                </span>
              )
            ) : isBalance ? (
              <span className="inline-flex items-center gap-1 text-xs text-zinc-700 dark:text-zinc-300">
                <span>{en ? 'Credited to Wallet' : '已入账至钱包'}</span>
                <Popover
                  title={<span className="text-zinc-900 dark:text-white font-semibold">{en ? 'Balance Voucher Details' : '余额卡券使用说明'}</span>}
                  content={
                    <div className="max-w-xs text-xs space-y-1.5 py-1 text-zinc-700 dark:text-zinc-300">
                      <div>
                        {(() => {
                          const sysAmt = Number(card.system_amount || 0);
                          const giftAmt = Number(card.gift_amount || 0);
                          if (sysAmt > 0 && giftAmt > 0) {
                            return en
                              ? `This voucher credited ${formatCreditMoney(symbol, sysAmt)} to system balance and ${formatCreditMoney(symbol, giftAmt)} to gift balance.`
                              : `该卡券已充入您的系统余额 ${formatCreditMoney(symbol, sysAmt)}、赠送金余额 ${formatCreditMoney(symbol, giftAmt)}。`;
                          }
                          if (giftAmt > 0) {
                            return en
                              ? `This voucher credited ${formatCreditMoney(symbol, giftAmt)} directly into your gift balance.`
                              : `该卡券面值 ${formatCreditMoney(symbol, giftAmt)} 已直接充入您的赠送金钱包余额。`;
                          }
                          return en
                            ? `This voucher credited ${formatCreditMoney(symbol, card.face_value)} directly into your account balance.`
                            : `该卡券面值 ${formatCreditMoney(symbol, card.face_value)} 已直接充入您的系统钱包余额。`;
                        })()}
                      </div>
                      <div className="text-zinc-500 dark:text-zinc-400">{en ? 'All model API calls and services draw automatically from your balance.' : '调用全平台 AI 模型及业务消费时将按次自动扣减余额。'}</div>
                      <Button type="link" size="small" className="p-0 h-auto text-zinc-600 hover:text-zinc-900 dark:text-zinc-300 dark:hover:text-white" onClick={onNavigateWallet}>
                        {en ? 'View Wallet Ledger →' : '查看钱包消费流水 →'}
                      </Button>
                    </div>
                  }
                >
                  <InfoCircleOutlined className="cursor-pointer opacity-70 hover:opacity-100 transition-opacity text-zinc-400" />
                </Popover>
              </span>
            ) : card.status === 'exhausted' ? (
              <span className="text-zinc-500 font-medium text-xs">{en ? 'Exhausted' : '已耗尽'}</span>
            ) : card.status === 'expired' ? (
              <span className="text-zinc-500 font-medium text-xs">{en ? 'Expired' : '已过期'}</span>
            ) : (
              /* 剩余可用金额 */
              <div className="flex items-baseline gap-1">
                <span className="text-xs text-zinc-500 dark:text-zinc-400">{en ? 'Rem:' : '剩余:'}</span>
                <span className="text-xs sm:text-sm font-semibold tracking-tight text-zinc-900 dark:text-zinc-100">
                  {formatCreditMoney(symbol, card.remaining)}
                </span>
              </div>
            )}
          </div>
        </div>

        {/* 顶部同款发光渐变进度条 (shadcn 纯黑白灰精炼进度条) */}
        <div className="w-full h-1.5 bg-zinc-200 dark:bg-zinc-800 rounded-full overflow-hidden relative">
          {isSpace ? (
            isActive && (
              <div
                className="h-full bg-zinc-200 dark:bg-zinc-100 rounded-full transition-all duration-500"
                style={{ width: '100%' }}
              />
            )
          ) : (
            percentRemaining > 0 && (
              <div
                className="h-full bg-zinc-200 dark:bg-zinc-100 rounded-full transition-all duration-500"
                style={{ width: `${percentRemaining}%` }}
              />
            )
          )}
        </div>
      </div>
    </div>
  );
};

const CouponRecords: React.FC = () => {
  const { i18n } = useTranslation();
  const en = (i18n.language || '').toLowerCase().startsWith('en');
  const navigate = useNavigate();
  const location = useLocation();
  const { user } = useAuthStore();
  const { settings } = useSettingsStore();
  const symbol = settings?.currency?.currency_symbol || '$';

  const [membershipOn, setMembershipOn] = useState(false);
  const [credits, setCredits] = useState<Credit[]>([]);
  const [memberPlans, setMemberPlans] = useState<MemberSub[] | null>(null);
  const [spaceGrants, setSpaceGrants] = useState<SpaceGrant[]>([]);
  const [vouchers, setVouchers] = useState<Voucher[]>([]);
  const [walletBalance, setWalletBalance] = useState<number | null>(null);

  // 筛选与视图状态 (默认展示“可用/已入账”分类，默认包含近3天失效/耗尽卡券)
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState<'all' | 'active' | 'exhausted' | 'expired'>('active');
  const [includeRecentInactive, setIncludeRecentInactive] = useState(true);
  const [sourceFilter, setSourceFilter] = useState<'all' | 'pack' | 'space' | 'redeem' | 'balance'>('all');

  // 新增：规则说明弹窗状态
  const [learnMoreOpen, setLearnMoreOpen] = useState(false);
  // 新增：首选生效营销活动
  const [activeCampaign, setActiveCampaign] = useState<{ id: number; template_key?: string } | null>(null);
  const [modelNameMap, setModelNameMap] = useState<Record<string, string>>({});

  // 兑换功能状态（支持弹窗与页面内联快速兑换）
  const [redeemModalOpen, setRedeemModalOpen] = useState(false);
  const [redeemCode, setRedeemCode] = useState('');
  const [redeeming, setRedeeming] = useState(false);
  const [redeemCooldown, setRedeemCooldown] = useState(0);
  const [redeemFeedback, setRedeemFeedback] = useState<{ type: 'success' | 'error' | 'warning'; text: string } | null>(null);
  const redeemLockRef = useRef(false);
  const redeemInputRef = useRef<HTMLInputElement>(null);
  const redemptionEnabled = settings?.marketing?.enable_redemption !== false;

  useEffect(() => {
    if (redeemCooldown <= 0) return;
    const timer = setTimeout(() => setRedeemCooldown((s) => s - 1), 1000);
    return () => clearTimeout(timer);
  }, [redeemCooldown]);

  const loadData = useCallback(async () => {
    try {
      const plugins = await fetchActivePlugins().catch(() => null);
      const matched = (plugins?.active_plugins || []).find((p: any) => p.name === 'marketing_membership');
      const pluginOn = !!matched;
      setMembershipOn(membershipVisible(matched, user));

      const tasks: Promise<void>[] = [
        // 拉取兑换卡券（包含模型卡券与余额卡券）
        (request.get('/user/model-vouchers', { skipErrorHandler: true } as any) as Promise<Voucher[]>)
          .then((rows) => setVouchers(Array.isArray(rows) ? rows : []))
          .catch(() => setVouchers([])),
        // 拉取钱包实时余额
        (request.get('/user/wallet', { skipErrorHandler: true } as any) as Promise<any>)
          .then((res) => {
            if (res?.balance != null) setWalletBalance(Number(res.balance));
          })
          .catch(() => {}),
        // 拉取模型列表以将 MID/Model ID 映射为模型名称
        (request.get('/models', { skipErrorHandler: true } as any) as Promise<{ data?: Array<{ mid?: string; model_id?: string; name?: string }> }>)
          .then((res) => {
            const map: Record<string, string> = {};
            (res?.data || []).forEach((item) => {
              const displayName = (item.name || '').trim() || (item.model_id || '').trim();
              if (!displayName) return;
              const mid = (item.mid || '').trim();
              const modelId = (item.model_id || '').trim();
              if (mid && !map[mid]) map[mid] = displayName;
              if (modelId && !map[modelId]) map[modelId] = displayName;
            });
            setModelNameMap(map);
          })
          .catch(() => {}),
      ];

      if (pluginOn) {
        tasks.push(
          (request.get('/plugins/marketing_membership/records', { skipErrorHandler: true } as any) as Promise<any>)
            .then((res) => {
              setCredits(res?.credits || []);
              setSpaceGrants(res?.space_grants || []);
              setMemberPlans(Array.isArray(res?.subscriptions) ? res.subscriptions : null);
            })
            .catch(() => {
              setCredits([]);
              setSpaceGrants([]);
              setMemberPlans(null);
            }),
        );
        tasks.push(
          (request.get('/plugins/marketing_membership/placements/console_nav_coupon', { skipErrorHandler: true } as any) as Promise<any>)
            .then((res) => {
              const list = Array.isArray(res?.campaigns) ? res.campaigns : (res?.campaign ? [res.campaign] : []);
              const first = list.find((row: any) => row?.id && (row?.plans || []).length > 0) || list.find((row: any) => row?.id);
              if (first?.id) {
                setActiveCampaign({ id: first.id, template_key: first.template_key });
              }
            })
            .catch(() => {}),
        );
      } else {
        setCredits([]);
        setSpaceGrants([]);
      }

      await Promise.all(tasks);
    } finally {
      // noop
    }
  }, [user]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // 合并购买与兑换的全部卡券（包含余额卡券、专属模型卡券、创作中心空间加油包与会员订阅）
  const resolveModelsList = useCallback(
    (list?: string[]) =>
      Array.from(
        new Set(
          (list || [])
            .map((m) => m.trim())
            .filter(Boolean)
            .map((m) => modelNameMap[m] || m),
        ),
      ),
    [modelNameMap],
  );

  const cards = useMemo<CardRow[]>(() => {
    const purchased: CardRow[] = credits.map((row) => {
      const isSub = row.offer_kind === 'subscription' || (!row.offer_kind && /订阅|subscription|包月|包季|包年|month|year/i.test(row.plan_name));
      return {
        key: `purchase-${row.id}`,
        source: isSub ? 'subscription' : 'pack',
        card_type: 'credit',
        name: row.plan_name,
        face_value: row.face_value,
        worth_value: row.worth_value,
        credit_discount: row.credit_discount,
        model_discounts: row.model_discounts,
        remaining: row.remaining,
        models: resolveModelsList(row.model_ids),
        expires_at: row.expires_at,
        status: row.status,
        created_at: row.created_at,
        updated_at: row.updated_at || row.created_at,
        plan_id: row.plan_id,
        billing_cycle: row.billing_cycle,
      };
    });

    const spaces: CardRow[] = [];
    spaceGrants.forEach((row) => {
      const isSub = row.offer_kind === 'subscription' || (!row.offer_kind && /订阅|subscription|包月|包季|包年|month|year/i.test(row.plan_name));
      if (isSub) {
        const matched = purchased.find((p) => p.source === 'subscription' && p.plan_id === row.plan_id);
        if (matched) {
          matched.space_mb = (matched.space_mb || 0) + row.space_mb;
          return;
        }
      }
      spaces.push({
        key: `space-${row.id}`,
        source: isSub ? 'subscription' : 'pack',
        card_type: 'space',
        name: row.plan_name,
        face_value: 0,
        remaining: 0,
        space_mb: row.space_mb,
        models: [],
        expires_at: row.expires_at,
        status: row.status,
        created_at: row.created_at,
        updated_at: row.updated_at || row.created_at,
        serial: row.order_no || '',
        plan_id: row.plan_id,
        billing_cycle: row.billing_cycle,
      });
    });

    const redeemed: CardRow[] = vouchers.map((row) => {
      if (row.source === 'playground_space') {
        return {
          key: `playground-space-${row.id}`,
          source: 'playground_space',
          card_type: 'space',
          name: row.name || (en ? 'Creative space' : '创作空间兑换'),
          description: row.description || '',
          face_value: 0,
          remaining: 0,
          space_mb: row.space_mb || 0,
          models: [],
          expires_at: row.expires_at,
          status: row.status,
          created_at: row.redeemed_at,
          updated_at: row.updated_at || row.redeemed_at,
          serial: row.code || '',
        };
      }
      const isBalance = row.source === 'balance';
      return {
        key: `${row.source || 'redeem'}-${row.id}`,
        source: isBalance ? 'balance' : 'redeem',
        card_type: 'credit',
        name: row.name || (isBalance ? (en ? 'Balance Voucher' : '余额卡券') : (en ? 'Model Voucher' : '专属卡券')),
        description: row.description || '',
        face_value: row.face_value,
        worth_value: row.worth_value,
        credit_discount: row.credit_discount,
        model_discounts: row.model_discounts,
        system_amount: row.system_amount,
        gift_amount: row.gift_amount,
        remaining: row.remaining,
        models: resolveModelsList(row.allowed_models),
        expires_at: row.expires_at,
        status: row.status,
        created_at: row.redeemed_at,
        updated_at: row.updated_at || row.redeemed_at,
        serial: row.code || '',
      };
    });

    return [...purchased, ...spaces, ...redeemed].sort((a, b) => String(b.created_at).localeCompare(String(a.created_at)));
  }, [credits, spaceGrants, vouchers, en, resolveModelsList]);

  // 会员订阅列表（置于顶部“订阅方案”直接列表展示）
  const memberSubscriptions = useMemo(() => {
    if (memberPlans) {
      return memberPlans.map((sub) => ({
        ...sub,
        models: resolveModelsList(sub.models),
      }));
    }
    return cards
      .filter((c) => c.source === 'subscription')
      .map((card): MemberSub => ({
        plan_id: card.plan_id || 0,
        name: card.name,
        status: card.status === 'active' || card.status === 'credited' ? 'active' : 'expired',
        expires_at: card.expires_at,
        billing_cycle: card.billing_cycle || 'month',
        renew_cycle: card.billing_cycle || 'month',
        renew_state: 'renew',
        credit_remaining: card.remaining,
        credit_face: card.face_value,
        credit_worth: card.worth_value,
        credit_discount: card.credit_discount,
        model_discounts: card.model_discounts,
        models: card.models,
        space_mb: card.space_mb || 0,
      }));
  }, [cards, memberPlans, resolveModelsList]);

  // 卡券与加油包明细列表（排除会员订阅，在下方卡券明细中展示）
  const couponCards = useMemo(() => {
    return cards.filter((c) => c.source !== 'subscription');
  }, [cards]);

  // 统计指标（全局可用卡券与特权统计，严格排除已充入钱包的余额券与存储空间扩容券）
  const activeCards = useMemo(() => cards.filter((c) => isCardActive(c) && c.source !== 'balance' && c.card_type !== 'space'), [cards]);

  // 卡券明细区专用状态统计（排除会员订阅；已使用兑换的余额卡券归入已耗尽）
  const couponActiveCards = useMemo(() => couponCards.filter((c) => isCardActive(c)), [couponCards]);
  const couponExhaustedCards = useMemo(() => couponCards.filter((c) => c.status === 'exhausted' || c.status === 'credited'), [couponCards]);
  const couponExpiredCards = useMemo(
    () => couponCards.filter((c) => c.status === 'expired' || c.status === 'voided' || c.status === 'returned' || (c.expires_at && new Date(c.expires_at).getTime() <= Date.now())),
    [couponCards],
  );
  const recentInactiveCount = useMemo(() => {
    return couponCards.filter((c) => !isCardActive(c) && isCardRecentExpiredOrExhausted(c)).length;
  }, [couponCards]);

  // 卡券明细区来源分类计数（加油包、空间券、专属模型券、余额券）
  const sourceCounts = useMemo(() => {
    let pack = 0;
    let space = 0;
    let redeem = 0;
    let balance = 0;
    couponCards.forEach((c) => {
      if (c.card_type === 'space') {
        space++;
        if (c.source === 'pack') pack++;
      } else if (c.source === 'pack') {
        pack++;
      } else if (c.source === 'redeem') {
        redeem++;
      } else if (c.source === 'balance') {
        balance++;
      }
    });
    return { pack, space, redeem, balance };
  }, [couponCards]);

  // 可用模型卡券剩余额度
  const totalModelRemaining = useMemo(() => {
    return activeCards.reduce((acc, c) => acc + (c.remaining || 0), 0);
  }, [activeCards]);

  // 累计卡券总面值（排除已充入钱包的余额卡券与存储空间扩容券，仅统计专属模型券、加油包与订阅）
  const totalFaceValue = useMemo(() => {
    return cards
      .filter((c) => c.source !== 'balance' && c.card_type !== 'space')
      .reduce((acc, c) => acc + (c.face_value || 0), 0);
  }, [cards]);

  // 累计有效创作中心额外空间配额 (MB)
  const totalActiveSpaceMb = useMemo(() => {
    const now = Date.now();
    const purchased = spaceGrants
      .filter((g) => g.status === 'active' && (!g.expires_at || new Date(g.expires_at).getTime() > now))
      .reduce((acc, g) => acc + (g.space_mb || 0), 0);
    const redeemed = cards
      .filter((c) => c.source === 'playground_space' && c.card_type === 'space' && isCardActive(c))
      .reduce((acc, c) => acc + (c.space_mb || 0), 0);
    return purchased + redeemed;
  }, [spaceGrants, cards]);

  // 收集当前活跃额度卡券中覆盖的模型范围（含全模型通用卡券）
  const activeCoveredModels = useMemo(() => {
    const set = new Set<string>();
    let hasAllModels = false;
    activeCards.forEach((c) => {
      if (c.card_type === 'space' || c.source === 'balance' || (c.remaining || 0) <= 0) {
        return;
      }
      const list = (c.models || []).map((m) => m.trim()).filter(Boolean);
      if (list.length === 0) {
        hasAllModels = true;
      } else {
        list.forEach((m) => set.add(m));
      }
    });
    const out = Array.from(set);
    if (hasAllModels) {
      out.unshift(en ? 'All Models (Universal)' : '全模型通用');
    }
    return out;
  }, [activeCards, en]);

  // 额度进度百分比
  const creditsPercent = useMemo(() => {
    if (totalFaceValue > 0) {
      return Math.min(100, Math.max(0, Math.round((totalModelRemaining / totalFaceValue) * 100)));
    }
    return 0;
  }, [totalFaceValue, totalModelRemaining]);

  const handleUpgrade = () => {
    if (activeCampaign?.id) {
      navigate(couponCampaignPath(activeCampaign.template_key, activeCampaign.id), {
        state: offerReturnState(`${location.pathname}${location.search}${location.hash}`),
      });
    } else {
      navigate('/credits', {
        state: offerReturnState(`${location.pathname}${location.search}${location.hash}`),
      });
    }
  };

  // 直接执行订阅周期的续费操作，跳转至专属结账单
  const handleRenew = (sub: MemberSub) => {
    const cycle = sub.renew_cycle || sub.billing_cycle || 'month';
    if (sub.plan_id && sub.renew_state !== 'off_shelf' && sub.renew_state !== 'unavailable') {
      navigate(`/orders/checkout/${sub.plan_id}?cycle=${cycle}`, {
        state: offerReturnState(`${location.pathname}${location.search}${location.hash}`),
      });
    } else {
      handleUpgrade();
    }
  };

  // 过滤后的卡券（仅从 couponCards 中筛选，不含会员订阅）
  const filteredCards = useMemo(() => {
    const list = couponCards.filter((card) => {
      if (statusFilter !== 'all') {
        if (statusFilter === 'active') {
          const isAct = isCardActive(card);
          const isRecent = includeRecentInactive && isCardRecentExpiredOrExhausted(card);
          if (!isAct && !isRecent) return false;
        } else if (statusFilter === 'exhausted') {
          if (!(card.status === 'exhausted' || card.status === 'credited')) return false;
        } else if (statusFilter === 'expired') {
          const isExp =
            card.status === 'expired' ||
            card.status === 'voided' ||
            card.status === 'returned' ||
            (card.expires_at && new Date(card.expires_at).getTime() <= Date.now());
          if (!isExp) return false;
        }
      }
      if (sourceFilter !== 'all') {
        if (sourceFilter === 'space') {
          if (card.card_type !== 'space') return false;
        } else if (sourceFilter === 'pack') {
          if (card.source !== 'pack') return false;
        } else if (card.source !== sourceFilter) {
          return false;
        }
      }
      if (search.trim()) {
        const query = search.trim().toLowerCase();
        const matchName = card.name.toLowerCase().includes(query);
        const matchSerial = (card.serial || '').toLowerCase().includes(query);
        const matchModels = card.models.some((m) => m.toLowerCase().includes(query));
        const matchSpace = card.space_mb != null && (
          `${card.space_mb}`.includes(query) ||
          `${card.space_mb}mb`.includes(query) ||
          '创作空间'.includes(query) ||
          '空间'.includes(query) ||
          'storage'.includes(query)
        );
        if (!matchName && !matchSerial && !matchModels && !matchSpace) return false;
      }
      return true;
    });

    if (statusFilter === 'active') {
      // 可用筛选下：生效中的卡券优先置顶，近3天已失效/耗尽的卡券排在下方，各自按最新时间倒序
      return list.sort((a, b) => {
        const aAct = isCardActive(a) ? 1 : 0;
        const bAct = isCardActive(b) ? 1 : 0;
        if (aAct !== bAct) return bAct - aAct;
        return String(b.created_at).localeCompare(String(a.created_at));
      });
    }

    return list;
  }, [couponCards, statusFilter, sourceFilter, search, includeRecentInactive]);

  // 聚焦快速兑换输入框并平滑滚动
  const focusRedeemInput = () => {
    const el = document.getElementById('coupon-redeem-section');
    if (el) {
      el.scrollIntoView({ behavior: 'smooth', block: 'center' });
      setTimeout(() => redeemInputRef.current?.focus(), 250);
    } else {
      setRedeemModalOpen(true);
    }
  };

  // 执行兑换
  const handleRedeem = async () => {
    if (redeemLockRef.current || redeeming || redeemCooldown > 0) return;
    const code = redeemCode.trim();
    if (!code) {
      setRedeemFeedback({
        type: 'warning',
        text: en ? 'Please enter a redemption code' : '请输入兑换码',
      });
      return;
    }
    redeemLockRef.current = true;
    setRedeeming(true);
    setRedeemFeedback(null);
    try {
      const res = (await request.post('/redemptions/redeem', { code }, {
        skipErrorHandler: true,
      } as any)) as any;
      if (res?.success) {
        const amount = Number(res.quota_added || 0);
        const giftAmount = Number(res.gift_quota_added || 0);
        const isVoucher = res.code_type === 'model_voucher';
        const isSpace = res.code_type === 'playground_space';
        const spaceMb = Math.round(Number(res.space_mb || amount || 0));
        const desc = typeof res.description === 'string' ? res.description.trim() : '';
        const balanceMsg = amount > 0 && giftAmount > 0
          ? (en
            ? `Redeemed successfully (+${symbol}${amount.toFixed(2)} system, +${symbol}${giftAmount.toFixed(2)} gift)`
            : `兑换成功，已充值系统余额 ${symbol}${amount.toFixed(2)}、赠送金余额 ${symbol}${giftAmount.toFixed(2)}`)
          : giftAmount > 0
            ? (en
              ? `Redeemed successfully (+${symbol}${giftAmount.toFixed(2)} gift balance)`
              : `兑换成功，已充值赠送金余额 ${symbol}${giftAmount.toFixed(2)}`)
            : (en
              ? `Redeemed successfully (+${symbol}${amount.toFixed(2)})`
              : `兑换成功，已充值 ${symbol}${amount.toFixed(2)}`);
        const baseMsg = isSpace
          ? (en
            ? `Creative Center storage increased by ${spaceMb} MB. Wallet balance is unchanged.`
            : `已增加创作中心存储空间 ${spaceMb} MB，账户余额不变。`)
          : isVoucher
          ? (en
            ? `Successfully redeemed model voucher: ${symbol}${amount.toFixed(2)}`
            : `成功兑换专属模型卡券: ${symbol}${amount.toFixed(2)}`)
          : balanceMsg;
        const fullMsg = desc ? `${baseMsg} · ${desc}` : baseMsg;
        message.success(fullMsg);
        setRedeemFeedback({
          type: 'success',
          text: fullMsg,
        });
        setRedeemModalOpen(false);
        setRedeemCode('');
        await loadData();
      }
    } catch (e: any) {
      const data = e?.response?.data;
      let serverMsg =
        data?.error?.message ||
        data?.message ||
        (typeof data?.error === 'string' ? data.error : undefined) ||
        (typeof data === 'string' ? data : undefined);
      if (serverMsg === 'Invalid or already used redemption code') {
        serverMsg = en ? 'Invalid or already used redemption code' : '兑换码无效或已被使用';
      }
      setRedeemFeedback({
        type: 'error',
        text: serverMsg || (en ? 'Redemption failed' : '兑换失败，请稍后重试'),
      });
    } finally {
      setRedeeming(false);
      setRedeemCooldown(3);
      setTimeout(() => {
        redeemLockRef.current = false;
      }, 300);
    }
  };

  return (
    <div className="w-full max-w-[1200px] mx-auto px-4 py-4 sm:py-6 text-zinc-900 dark:text-zinc-100 font-sans">
      {/* 页面主标题区 (统一遵循个人中心/用户中心 PageHeader 规范) */}
      <PageHeader
        title={en ? 'Subscription' : '卡券订阅'}
        description={
          en
            ? 'Manage your subscription privileges, voucher quotas, and privilege history.'
            : '管理您的会员订阅、卡券额度与特权使用明细。'
        }
        marginBottom={24}
      />

      {/* 页面四大主模块（上下间距统一调整为紧凑的 10px） */}
      <div className="space-y-[10px]">
        {/* 1. Subscription Card (订阅方案及全部会员订阅列表，仅在启用营销卡券会员插件或存在订阅记录时展示) */}
        {(membershipOn || memberSubscriptions.length > 0) && (
        <div className="rounded-2xl border border-zinc-200 dark:border-[#1f1f23] bg-white dark:bg-[#141414] p-4 sm:p-5 shadow-xs space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2 text-xs font-semibold text-zinc-500 dark:text-zinc-400">
            <svg className="w-3.5 h-3.5 text-zinc-400 shrink-0" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 2L2 9l10 13 10-13-10-7z" />
            </svg>
            <span>{en ? 'Subscription' : '订阅方案'}</span>
            {memberSubscriptions.length > 0 && (
              <span className="ml-1 px-2 py-0.5 rounded-full text-[10px] bg-zinc-100 dark:bg-[#1c1c1f] text-zinc-600 dark:text-zinc-300 border border-zinc-200 dark:border-zinc-700/60">
                {memberSubscriptions.length}
              </span>
            )}
          </div>
        </div>

        {/* 订阅方案列表展示 */}
        {memberSubscriptions.length === 0 ? (
          <div className="rounded-xl border border-zinc-200 dark:border-zinc-800/80 bg-[#fafafa] dark:bg-[#1c1c1f] px-4 py-3 sm:px-5 sm:py-3.5">
            <div className="text-sm font-semibold text-zinc-900 dark:text-white tracking-tight">
              {en ? 'Free Plan' : '免费方案'}
            </div>
            <div className="text-xs text-zinc-500 mt-0.5">
              {en ? 'No active subscriptions' : '暂无生效中的会员订阅'}
            </div>
          </div>
        ) : (
          <div className="space-y-2">
            {memberSubscriptions.map((sub) => {
              const isActive = sub.status === 'active';
              const offShelf = sub.renew_state === 'off_shelf';
              const canContinue = sub.renew_state === 'renew' || sub.renew_state === 'reopen' || sub.renew_state === 'cycle_gone';
              const isExpiringSoon = (() => {
                if (!sub.expires_at || !isActive) return false;
                const diff = new Date(sub.expires_at).getTime() - Date.now();
                return diff > 0 && diff <= 3 * 24 * 3600 * 1000;
              })();
              const renewLabel = sub.renew_state === 'reopen'
                ? (en ? 'Reopen' : '重新开通')
                : sub.renew_state === 'cycle_gone'
                  ? (en ? 'Change cycle' : '改选周期')
                  : sub.content_changed
                    ? (en ? 'Renew new plan' : '新方案续费')
                    : (en ? 'Renew' : '续费');

              return (
                <div
                  key={sub.plan_id}
                  className={`rounded-xl border py-2 px-3.5 sm:py-2 sm:px-4 transition-all flex flex-col gap-2 ${
                    isActive
                      ? 'border-zinc-200 dark:border-zinc-800 bg-[#fafafa] dark:bg-[#1c1c1f] hover:border-zinc-300 dark:hover:border-zinc-700 shadow-xs'
                      : 'border-zinc-200/60 dark:border-zinc-800/40 bg-zinc-100/50 dark:bg-[#141414]/60'
                  }`}
                >
                  <div className="flex flex-col md:flex-row md:items-center justify-between gap-2">
                  <div className="space-y-0.5 flex-1 min-w-0">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="flex items-center gap-1.5 text-xs font-medium text-zinc-700 dark:text-zinc-300">
                        <CrownOutlined className="text-zinc-400" />
                        <span>{en ? 'MEMBERSHIP' : '会员订阅'}</span>
                      </span>
                      <span className="text-sm font-semibold text-zinc-900 dark:text-white tracking-tight truncate">
                        {sub.name}
                      </span>
                      {offShelf ? (
                        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-zinc-100 dark:bg-zinc-800 text-zinc-500 dark:text-zinc-400 border border-zinc-200 dark:border-zinc-700/60">
                          {en ? 'OFF SHELF' : '已下架'}
                        </span>
                      ) : null}
                      {isActive ? (
                        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-zinc-100 dark:bg-zinc-800 text-zinc-800 dark:text-zinc-200 border border-zinc-200 dark:border-zinc-700/60">
                          <span className="w-1.5 h-1.5 rounded-full bg-zinc-500 dark:bg-zinc-400" />
                          {en ? 'ACTIVE' : '生效中'}
                        </span>
                      ) : (
                        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-zinc-100 dark:bg-zinc-800/40 text-zinc-500 border border-zinc-200 dark:border-zinc-800">
                          <span className="w-1.5 h-1.5 rounded-full bg-zinc-400 dark:bg-zinc-600" />
                          {en ? 'EXPIRED' : '已过期'}
                        </span>
                      )}
                      {isExpiringSoon && (
                        <span className="inline-flex items-center px-2 py-0.5 rounded-md text-[10px] font-medium bg-zinc-800 text-amber-400/90 border border-amber-500/20">
                          {en ? 'Expiring soon' : '即将到期'}
                        </span>
                      )}
                    </div>

                    <div className="flex flex-wrap items-center gap-x-3.5 gap-y-0.5 text-xs text-zinc-500 dark:text-zinc-400">
                      <div>
                        {en ? 'Expires: ' : '有效期至：'}
                        <span className={`font-semibold ${isExpiringSoon ? 'text-amber-500 dark:text-amber-400/90' : 'text-zinc-700 dark:text-zinc-200'}`}>
                          {sub.expires_at ? formatApiDateTime(sub.expires_at, 'YYYY-MM-DD HH:mm') : (en ? 'Permanent' : '长期有效')}
                        </span>
                      </div>
                      {sub.credit_face > 0 && (
                        <div>
                          {en ? 'Credits: ' : '模型额度：'}
                          <span className="text-zinc-700 dark:text-zinc-200 font-medium">
                            {formatCreditMoney(symbol, sub.credit_remaining)} / {formatCreditMoney(symbol, sub.credit_face)}
                          </span>
                          {(sub.credit_worth || 0) > sub.credit_face && (
                            <span className="ml-1 text-zinc-400">
                              {en ? `(Worth ${formatCreditMoney(symbol, sub.credit_worth || 0)})` : `(价值 ${formatCreditMoney(symbol, sub.credit_worth || 0)})`}
                            </span>
                          )}
                          {sub.credit_discount != null && sub.credit_discount > 0 && sub.credit_discount < 1 && (
                            <span className="ml-1 text-zinc-600 dark:text-zinc-300">
                              {en ? `· ${Math.round(sub.credit_discount * 100)}%` : `· ${Number((sub.credit_discount * 10).toFixed(2))}折`}
                            </span>
                          )}
                        </div>
                      )}
                      {sub.space_mb > 0 ? (
                        <div className="flex items-center gap-1">
                          <span className="text-zinc-500 text-xs">{en ? 'Storage:' : '空间:'}</span>
                          <span className="text-zinc-700 dark:text-zinc-300 font-medium">
                            +{sub.space_mb} MB
                          </span>
                        </div>
                      ) : null}
                      {sub.models && sub.models.length > 0 ? (
                        <Popover
                          title={<span className="text-zinc-900 dark:text-white font-semibold">{en ? 'Applicable Models' : '适用模型范围'}</span>}
                          content={
                            <div className="max-w-xs max-h-48 overflow-y-auto space-y-1 py-1">
                              {sub.models.map((m) => (
                                <div key={m} className="text-xs text-zinc-700 dark:text-zinc-300 flex items-center gap-1.5">
                                  <span className="w-1 h-1 rounded-full bg-zinc-400" />
                                  <span>{m}</span>
                                </div>
                              ))}
                            </div>
                          }
                        >
                          <span className="cursor-pointer text-zinc-600 dark:text-zinc-300 hover:text-zinc-900 dark:hover:text-white underline decoration-dotted underline-offset-2">
                            {en ? 'Models: ' : '适用模型：'}
                            {sub.models.length <= 2 ? sub.models.join(', ') : `${sub.models[0]} +${sub.models.length - 1}`}
                          </span>
                        </Popover>
                      ) : null}
                    </div>
                  </div>

                  <div className="shrink-0 flex items-center gap-2 self-start md:self-auto">
                    {canContinue ? (
                      <button
                        type="button"
                        onClick={() => handleRenew(sub)}
                        className="px-3 py-1.5 rounded-xl text-xs font-medium text-zinc-700 dark:text-zinc-200 bg-zinc-100 dark:bg-zinc-800 hover:bg-zinc-200 dark:hover:bg-zinc-700 border border-zinc-200 dark:border-zinc-700/60 active:scale-[0.98] transition-all shadow-xs cursor-pointer"
                      >
                        {renewLabel}
                      </button>
                    ) : (
                      <span className="text-xs text-zinc-500">
                        {sub.renew_state === 'unavailable'
                          ? (en ? 'No cycle on sale' : '暂无可售周期')
                          : (en ? 'Renewal closed' : '已停止续费')}
                      </span>
                    )}
                  </div>
                  </div>
                  {offShelf ? (
                    <div className="text-xs text-zinc-400">
                      {isActive
                        ? (en ? 'This plan is off the shelf. Benefits you already have stay until they expire.' : '方案已下架。已经买到的额度、等级和空间继续用到各自的到期时间，不能续费。')
                        : (en ? 'This plan is off the shelf and cannot be reopened.' : '方案已下架，不能重新开通。')}
                    </div>
                  ) : null}
                  {sub.renew_state === 'cycle_gone' ? (
                    <div className="text-xs text-amber-400/90">
                      {en ? 'Your previous cycle is no longer sold. You can pick a cycle that is still on sale.' : '原来的周期已停售，可以改选现在仍在卖的周期。'}
                    </div>
                  ) : null}
                  {sub.next_credit_face != null && sub.next_credit_at ? (
                    <div className="text-xs text-zinc-400">
                      {en
                        ? `Current credits stay until ${formatApiDateTime(sub.next_credit_at, 'YYYY-MM-DD HH:mm')}. The next ${formatCreditMoney(symbol, sub.next_credit_face)} starts then.`
                        : `当前额度保持到 ${formatApiDateTime(sub.next_credit_at, 'YYYY-MM-DD HH:mm')}。下一期 ${formatCreditMoney(symbol, sub.next_credit_face)} 到那时才发放。`}
                    </div>
                  ) : null}
                  {sub.next_space_mb != null && sub.next_space_at ? (
                    <div className="text-xs text-zinc-400">
                      {en
                        ? `Storage stays ${sub.space_mb} MB until ${formatApiDateTime(sub.next_space_at, 'YYYY-MM-DD HH:mm')}, then becomes ${sub.next_space_mb} MB.`
                        : `空间将于 ${formatApiDateTime(sub.next_space_at, 'YYYY-MM-DD HH:mm')} 调整为 ${sub.next_space_mb} MB。在此之前仍按 ${sub.space_mb} MB。`}
                    </div>
                  ) : null}
                  {sub.content_changed && canContinue ? (
                    <div className="text-[11px] text-zinc-400">
                      <div>{en ? 'Plan updated: takes effect next cycle, current period unaffected.' : '方案已更新，下期生效，本期不受影响。'}</div>
                    </div>
                  ) : null}
                </div>
              );
            })}
          </div>
        )}
      </div>
        )}

      {/* 2. 兑换码充值卡片 (从我的钱包迁移至卡券额度上方) */}
      {redemptionEnabled && (
        <div
          id="coupon-redeem-section"
          className="rounded-2xl border border-zinc-200 dark:border-[#1f1f23] bg-white dark:bg-[#141414] p-4 sm:p-5 shadow-xs"
        >
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex flex-wrap items-center gap-3">
              <div className="flex items-center gap-2">
                <TagOutlined className="text-zinc-500 dark:text-zinc-400 text-sm shrink-0" />
                <span className="text-xs sm:text-sm font-semibold text-zinc-900 dark:text-white shrink-0">
                  {en ? 'Redeem Code' : '兑换码充值'}
                </span>
              </div>

              <div className="flex items-center gap-2">
                <input
                  ref={redeemInputRef}
                  type="text"
                  value={redeemCode}
                  onChange={(e) => {
                    setRedeemCode(e.target.value.trim());
                    if (redeemFeedback) setRedeemFeedback(null);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') handleRedeem();
                  }}
                  placeholder={en ? 'Enter redemption code' : '请输入兑换码'}
                  className="bg-[#fafafa] dark:bg-[#1c1c1f] border border-zinc-200 dark:border-zinc-800 hover:border-zinc-300 dark:hover:border-zinc-700 focus:border-zinc-400 dark:focus:border-zinc-600 rounded-xl px-3.5 py-1.5 text-xs text-zinc-900 dark:text-white placeholder:text-zinc-400 dark:placeholder:text-zinc-500 focus:outline-none w-56 sm:w-64 transition-colors font-sans"
                />
                <Button
                  type="primary"
                  loading={redeeming}
                  disabled={redeeming || redeemCooldown > 0 || !redeemCode.trim()}
                  onClick={handleRedeem}
                  className="bg-zinc-900! text-white! hover:bg-zinc-800! dark:bg-zinc-100! dark:text-zinc-900! dark:hover:bg-white! border-none! font-medium shadow-xs text-xs px-4 h-[32px] rounded-xl shrink-0"
                >
                  {redeemCooldown > 0
                    ? (en ? `Wait ${redeemCooldown}s` : `${redeemCooldown}s 后再试`)
                    : (en ? 'Redeem' : '兑换')}
                </Button>
              </div>
            </div>

            {redeemFeedback && (
              <span
                className={`text-xs font-medium ${
                  redeemFeedback.type === 'success'
                    ? 'text-emerald-600 dark:text-emerald-400'
                    : redeemFeedback.type === 'warning'
                    ? 'text-amber-600 dark:text-amber-400'
                    : 'text-red-500 dark:text-red-400'
                }`}
              >
                {redeemFeedback.text}
              </span>
            )}
          </div>
        </div>
      )}

      {/* 3. Credits Card */}
      <div className="rounded-2xl border border-zinc-200 dark:border-[#1f1f23] bg-white dark:bg-[#141414] p-4 sm:p-5 shadow-xs">
        <div className="flex items-center justify-between mb-2.5">
          <div className="flex items-center gap-2 text-xs font-semibold text-zinc-500 dark:text-zinc-400">
            <svg className="w-3.5 h-3.5 text-zinc-400 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="9" cy="9" r="6" />
              <path d="M15 9a6 6 0 1 1-6 6" />
            </svg>
            <span>{en ? 'Credits' : '卡券额度'}</span>
          </div>
          <button
            type="button"
            onClick={() => setLearnMoreOpen(true)}
            className="text-xs text-zinc-600 dark:text-zinc-300 bg-zinc-100 dark:bg-zinc-800 hover:bg-zinc-200 dark:hover:bg-zinc-700 border border-zinc-200 dark:border-zinc-700/60 px-2.5 py-0.5 rounded-md transition-colors cursor-pointer"
          >
            {en ? 'Learn more' : '了解规则'}
          </button>
        </div>

        <div className="rounded-xl border border-zinc-200 dark:border-zinc-800/80 bg-[#fafafa] dark:bg-[#1c1c1f] p-3.5 sm:p-4">
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
            <div>
              <div className="text-xs text-zinc-500 dark:text-zinc-400 font-medium">
                {en ? 'Available Credits' : '可用卡券余额'}
              </div>
              <div className="mt-0.5 text-base sm:text-lg font-semibold text-zinc-900 dark:text-white tracking-tight">
                {formatCreditMoney(symbol, totalModelRemaining)} / {formatCreditMoney(symbol, totalFaceValue)}
              </div>
              {totalActiveSpaceMb > 0 && (
                <div className="mt-2 flex items-center gap-1.5 text-xs text-zinc-400 flex-wrap">
                  <CloudOutlined className="text-zinc-400" />
                  <span>{en ? 'Creative Storage:' : '创作空间:'}</span>
                  <span className="text-zinc-200 font-medium">+{totalActiveSpaceMb} MB</span>
                  <button
                    type="button"
                    className="text-xs text-zinc-400 hover:text-zinc-200 underline decoration-dotted underline-offset-2 cursor-pointer bg-transparent border-0 p-0 ml-1"
                    onClick={() => navigate('/playground-2026')}
                  >
                    {en ? 'Open Playground →' : '前往创作中心 →'}
                  </button>
                </div>
              )}
            </div>
          </div>

          <div className="mt-3 w-full">
            <div className="w-full h-1.5 bg-zinc-200 dark:bg-zinc-800 rounded-full overflow-hidden relative">
              {creditsPercent > 0 && (
                <div
                  className="h-full bg-zinc-200 dark:bg-zinc-100 rounded-full transition-all duration-500"
                  style={{
                    width: `${creditsPercent}%`,
                  }}
                />
              )}
            </div>
          </div>
        </div>
      </div>

      {/* 3. Active Covered Models Card */}
      <div className="rounded-2xl border border-zinc-200 dark:border-[#1f1f23] bg-white dark:bg-[#141414] p-4 sm:p-5 shadow-xs">
        <div className="flex items-center gap-2 text-xs font-semibold text-zinc-500 dark:text-zinc-400 mb-2.5">
          <AppstoreOutlined className="text-sm text-zinc-400" />
          <span>{en ? 'Covered models (active credits)' : '当前卡券可用模型'}</span>
          {activeCoveredModels.length > 0 && (
            <span className="ml-1 px-2 py-0.5 rounded-full text-[10px] bg-zinc-100 dark:bg-[#1c1c1f] text-zinc-600 dark:text-zinc-300 border border-zinc-200 dark:border-zinc-700/60">
              {activeCoveredModels.length}
            </span>
          )}
        </div>

        {/* Empty or Active Models State */}
        {activeCoveredModels.length === 0 ? (
          <div className="rounded-xl border border-zinc-200 dark:border-zinc-800/80 bg-[#fafafa] dark:bg-[#1c1c1f] p-4 sm:p-5 flex flex-col items-center justify-center text-center">
            <svg className="w-7 h-7 text-zinc-400 dark:text-zinc-500 mb-1.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="22 12 16 12 14 15 10 15 8 12 2 12" />
              <path d="M5.45 5.11L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />
            </svg>
            <div className="text-xs font-semibold text-zinc-500 dark:text-zinc-400">
              {en ? 'No active model credits available' : '暂无可用卡券模型额度'}
            </div>
          </div>
        ) : (
          <div className="rounded-xl border border-zinc-200 dark:border-zinc-800/80 bg-[#fafafa] dark:bg-[#1c1c1f] p-3.5 sm:p-4">
            <div className="flex flex-wrap gap-1.5 sm:gap-2">
              {activeCoveredModels.map((m) => (
                <span
                  key={m}
                  className="px-2 py-0.5 rounded-md text-[11px] font-medium bg-zinc-100 dark:bg-zinc-800 text-zinc-700 dark:text-zinc-300 border border-zinc-200 dark:border-zinc-700/60 flex items-center gap-1.5"
                >
                  <span className="w-1.5 h-1.5 rounded-full bg-zinc-400 dark:bg-zinc-500 shrink-0" />
                  {m}
                </span>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* 4. 卡券明细记录卡片（直接默认展示） */}
      <div id="coupon-history-section" className="rounded-2xl border border-zinc-200 dark:border-[#1f1f23] bg-white dark:bg-[#141414] p-4 sm:p-5 shadow-xs space-y-3">
          {/* 卡片头部 */}
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2.5">
            <div className="flex items-center gap-2 text-xs font-semibold text-zinc-500 dark:text-zinc-400">
              <HistoryOutlined className="text-sm text-zinc-400" />
              <span>{en ? 'Vouchers & Fuel Packs History' : '卡券明细与特权历史'}</span>
              <span className="ml-1 px-2 py-0.5 rounded-full text-[10px] bg-zinc-100 dark:bg-[#1c1c1f] text-zinc-600 dark:text-zinc-300 border border-zinc-200 dark:border-zinc-700/60">
                {filteredCards.length} / {couponCards.length}
              </span>
            </div>

            {/* 右侧工具：搜索框 */}
            <div className="flex items-center gap-2">
              <input
                type="text"
                placeholder={en ? 'Search vouchers...' : '搜索卡券 / 兑换码...'}
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                className="bg-[#fafafa] dark:bg-[#1c1c1f] border border-zinc-200 dark:border-zinc-800 hover:border-zinc-300 dark:hover:border-zinc-700 focus:border-zinc-400 dark:focus:border-zinc-600 rounded-xl px-3 py-1.5 text-xs text-zinc-900 dark:text-white placeholder:text-zinc-400 dark:placeholder:text-zinc-500 focus:outline-none w-44 sm:w-56 transition-colors font-sans"
              />
            </div>
          </div>

          {/* 筛选标签条（暗黑药丸式） */}
          <div className="flex flex-wrap items-center justify-between gap-2 pt-2 border-t border-zinc-200 dark:border-zinc-800/80">
            <div className="flex flex-wrap items-center gap-2">
              {/* 1. 卡券类型/来源筛选 (不含会员订阅，支持加油包、空间券、模型券、余额券) */}
              <div className="flex items-center gap-1 p-1 bg-[#fafafa] dark:bg-[#1c1c1f] rounded-xl border border-zinc-200 dark:border-zinc-800">
                {[
                  { label: `${en ? 'All' : '全部'} (${couponCards.length})`, value: 'all' },
                  ...(membershipOn || sourceCounts.pack > 0
                    ? [{ label: `${en ? 'Fuel Packs' : '加油包'} (${sourceCounts.pack})`, value: 'pack' }]
                    : []),
                  ...(membershipOn || sourceCounts.space > 0
                    ? [{ label: `${en ? 'Storage' : '空间券'} (${sourceCounts.space})`, value: 'space' }]
                    : []),
                  { label: `${en ? 'Models' : '模型券'} (${sourceCounts.redeem})`, value: 'redeem' },
                  { label: `${en ? 'Balance' : '余额券'} (${sourceCounts.balance})`, value: 'balance' },
                ].map((item) => {
                  const isSelected = sourceFilter === item.value;
                  return (
                    <button
                      key={item.value}
                      type="button"
                      onClick={() => setSourceFilter(item.value as any)}
                      className={`px-3 py-1 rounded-lg text-xs font-medium transition-all cursor-pointer ${
                        isSelected
                          ? 'bg-white dark:bg-zinc-800 text-zinc-900 dark:text-white shadow-xs font-semibold border border-zinc-200 dark:border-zinc-700/60'
                          : 'text-zinc-500 dark:text-zinc-400 hover:text-zinc-900 dark:hover:text-zinc-200'
                      }`}
                    >
                      {item.label}
                    </button>
                  );
                })}
              </div>

              {/* 2. 状态筛选 */}
              <div className="flex items-center gap-1 p-1 bg-[#fafafa] dark:bg-[#1c1c1f] rounded-xl border border-zinc-200 dark:border-zinc-800">
                {[
                  { label: `${en ? 'All' : '全部'} (${couponCards.length})`, value: 'all' },
                  { label: `${en ? 'Active' : '可用'} (${couponActiveCards.length})`, value: 'active' },
                  { label: `${en ? 'Used' : '已耗尽'} (${couponExhaustedCards.length})`, value: 'exhausted' },
                  { label: `${en ? 'Expired' : '已失效'} (${couponExpiredCards.length})`, value: 'expired' },
                ].map((item) => {
                  const isSelected = statusFilter === item.value;
                  return (
                    <button
                      key={item.value}
                      type="button"
                      onClick={() => setStatusFilter(item.value as any)}
                      className={`px-3 py-1 rounded-lg text-xs font-medium transition-all cursor-pointer ${
                        isSelected
                          ? 'bg-white dark:bg-zinc-800 text-zinc-900 dark:text-white shadow-xs font-semibold border border-zinc-200 dark:border-zinc-700/60'
                          : 'text-zinc-500 dark:text-zinc-400 hover:text-zinc-900 dark:hover:text-zinc-200'
                      }`}
                    >
                      {item.label}
                    </button>
                  );
                })}
              </div>

              {/* 当选中“可用”筛选时，展示“含近3天失效/耗尽”选项（默认勾选开启） */}
              {statusFilter === 'active' && (
                <label className="flex items-center gap-1.5 px-2.5 py-1 bg-[#fafafa] dark:bg-[#1c1c1f] hover:bg-zinc-100 dark:hover:bg-zinc-800/60 rounded-xl border border-zinc-200 dark:border-zinc-800 text-xs text-zinc-600 dark:text-zinc-300 cursor-pointer select-none transition-colors">
                  <input
                    type="checkbox"
                    checked={includeRecentInactive}
                    onChange={(e) => setIncludeRecentInactive(e.target.checked)}
                    className="accent-zinc-900 dark:accent-zinc-100 rounded cursor-pointer w-3.5 h-3.5"
                  />
                  <span className="text-[11px] sm:text-xs text-zinc-600 dark:text-zinc-300">
                    {en
                      ? `Past 3 days (${recentInactiveCount})`
                      : `含近 3 天失效 (${recentInactiveCount})`}
                  </span>
                </label>
              )}
            </div>
          </div>

          {/* 卡券明细列表（深色横向行列表） */}
          {filteredCards.length === 0 ? (
            <div className="rounded-xl border border-zinc-200 dark:border-zinc-800/80 bg-[#fafafa] dark:bg-[#1c1c1f] p-8 flex flex-col items-center justify-center text-center">
              <svg className="w-8 h-8 text-zinc-400 dark:text-zinc-600 mb-2" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                <polyline points="22 12 16 12 14 15 10 15 8 12 2 12" />
                <path d="M5.45 5.11L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />
              </svg>
              <div className="text-xs font-semibold text-zinc-500 dark:text-zinc-400 mb-3">
                {en ? 'No coupon records found' : '暂无符合条件的卡券记录'}
              </div>
              <Button
                type="primary"
                size="small"
                icon={<GiftOutlined />}
                onClick={focusRedeemInput}
                className="bg-zinc-900! text-white! hover:bg-zinc-800! dark:bg-zinc-100! dark:text-zinc-900! dark:hover:bg-white! border-none! font-medium shadow-xs"
              >
                {en ? 'Redeem Voucher' : '兑换卡券'}
              </Button>
            </div>
          ) : (
            <div className="space-y-1.5">
              {filteredCards.map((card) => (
                <CouponCard
                  key={card.key}
                  card={card}
                  symbol={symbol}
                  en={en}
                  onNavigateWallet={() => navigate('/wallet')}
                />
              ))}
            </div>
          )}
        </div>
      </div>

      {/* 规则说明弹窗 */}
      <Modal
        open={learnMoreOpen}
        onCancel={() => setLearnMoreOpen(false)}
        footer={[
          <Button
            key="close"
            type="primary"
            onClick={() => setLearnMoreOpen(false)}
            className="bg-zinc-900! text-white! hover:bg-zinc-800! dark:bg-zinc-100! dark:text-zinc-900! dark:hover:bg-white! border-none! font-medium shadow-xs"
          >
            {en ? 'Got it' : '知道了'}
          </Button>,
        ]}
        title={
          <div className="flex items-center gap-2">
            <InfoCircleOutlined className="text-zinc-400" />
            <span>{membershipOn ? (en ? 'Subscription & Credits Rules' : '订阅与卡券规则说明') : (en ? 'Credits & Vouchers Rules' : '卡券额度规则说明')}</span>
          </div>
        }
        centered
        width={480}
      >
        <div className="space-y-4 py-3 text-sm text-zinc-700 dark:text-zinc-300">
          {membershipOn && (
            <div className="rounded-lg bg-[#fafafa] dark:bg-[#141414] p-3.5 border border-zinc-200 dark:border-zinc-800">
              <h4 className="font-semibold text-zinc-900 dark:text-white mb-1 flex items-center gap-1.5">
                <CrownOutlined className="text-zinc-400" />
                <span>{en ? '1. Subscription Plans' : '1. 订阅方案与会员特权'}</span>
              </h4>
              <p className="text-xs text-zinc-600 dark:text-zinc-400 leading-relaxed m-0">
                {en
                  ? 'Subscribing to a plan grants membership tier discounts, dedicated model credits, and workspace storage expansion for the duration of your membership.'
                  : '订购会员卡券可在有效期内享有会员等级专属折扣、指定或全模型消费额度及创作空间扩容特权。'}
              </p>
            </div>
          )}

          <div className="rounded-lg bg-[#fafafa] dark:bg-[#141414] p-3.5 border border-zinc-200 dark:border-zinc-800">
            <h4 className="font-semibold text-zinc-900 dark:text-white mb-1 flex items-center gap-1.5">
              <ThunderboltOutlined className="text-zinc-400" />
              <span>{membershipOn ? (en ? '2. Model Credits Priority' : '2. 专属模型额度优先抵扣') : (en ? '1. Model Credits Priority' : '1. 专属模型额度优先抵扣')}</span>
            </h4>
            <p className="text-xs text-zinc-600 dark:text-zinc-400 leading-relaxed m-0">
              {en
                ? 'When calling AI models, available model voucher quotas are deducted first. Once exhausted, calls fall back to your wallet balance.'
                : '发起模型调用或多模态任务时，系统优先消耗对应模型可用的卡券专属额度；卡券额度耗尽或不匹配时，自动扣减钱包可用余额。'}
            </p>
          </div>

          <div className="rounded-lg bg-[#fafafa] dark:bg-[#141414] p-3.5 border border-zinc-200 dark:border-zinc-800">
            <h4 className="font-semibold text-zinc-900 dark:text-white mb-1 flex items-center gap-1.5">
              <WalletOutlined className="text-zinc-400" />
              <span>{membershipOn ? (en ? '3. Balance Vouchers' : '3. 余额卡券与充值') : (en ? '2. Balance Vouchers' : '2. 余额卡券与充值')}</span>
            </h4>
            <p className="text-xs text-zinc-600 dark:text-zinc-400 leading-relaxed m-0">
              {en
                ? 'Balance vouchers are 100% credited to your platform wallet balance upon redemption and can be used across all services.'
                : '兑换的余额卡券将 100% 充值入账至您的平台钱包，可用于全平台模型调用与插件消费。'}
            </p>
          </div>

          {membershipOn && (
            <div className="rounded-lg bg-[#fafafa] dark:bg-[#141414] p-3.5 border border-zinc-200 dark:border-zinc-800">
              <h4 className="font-semibold text-zinc-900 dark:text-white mb-1 flex items-center gap-1.5">
                <CloudOutlined className="text-zinc-400" />
                <span>{en ? '4. Creative Storage Packs' : '4. 创作中心存储空间'}</span>
              </h4>
              <p className="text-xs text-zinc-600 dark:text-zinc-400 leading-relaxed m-0">
                {en
                  ? 'Storage fuel packs expand your dedicated cloud storage in Creative Center. Multiple packs stack automatically during their validity period.'
                  : '购买存储空间加油包或含空间特权的卡券方案，可在有效期内为创作中心叠加扩充专属云端存储配额；多笔购买自动累加生效。'}
              </p>
            </div>
          )}
        </div>
      </Modal>

      {/* 快捷兑换卡券弹窗 (shadcn 规范原生 Modal) */}
      <Modal
        open={redeemModalOpen}
        onCancel={() => {
          setRedeemModalOpen(false);
          setRedeemCode('');
          setRedeemFeedback(null);
        }}
        footer={null}
        destroyOnClose
        centered
        width={420}
        title={
          <div className="flex items-center gap-2 pb-0.5">
            <div className="w-7 h-7 rounded-md bg-[#fafafa] dark:bg-[#141414] text-foreground flex items-center justify-center border border-zinc-200 dark:border-zinc-800">
              <GiftOutlined className="text-sm text-zinc-600 dark:text-zinc-300" />
            </div>
            <div className="text-base font-semibold text-foreground">
              {en ? 'Redeem Coupon' : '兑换卡券'}
            </div>
          </div>
        }
      >
        <div className="pt-2.5 space-y-3.5">
          {redeemFeedback && (
            <Alert
              type={redeemFeedback.type}
              showIcon
              message={redeemFeedback.text}
              closable
              onClose={() => setRedeemFeedback(null)}
            />
          )}

          <div className="space-y-1">
            <Input
              size="middle"
              placeholder={en ? 'Enter coupon or redemption code...' : '请输入卡券或兑换码...'}
              value={redeemCode}
              onChange={(e) => setRedeemCode(e.target.value.trim())}
              onPressEnter={handleRedeem}
              disabled={redeeming}
              allowClear
              autoFocus
            />
          </div>

          <div className="flex items-center justify-end gap-2 pt-2 border-t border-zinc-200 dark:border-zinc-800">
            <Button
              onClick={() => {
                setRedeemModalOpen(false);
                setRedeemCode('');
                setRedeemFeedback(null);
              }}
              size="middle"
              className="bg-zinc-100 dark:bg-zinc-800 text-zinc-700 dark:text-zinc-300 border-zinc-200 dark:border-zinc-700/60 hover:bg-zinc-200 dark:hover:bg-zinc-700 hover:text-zinc-900 dark:hover:text-white"
            >
              {en ? 'Cancel' : '取消'}
            </Button>
            <Button
              type="primary"
              loading={redeeming}
              onClick={handleRedeem}
              icon={<CheckOutlined />}
              size="middle"
              className="bg-zinc-900! text-white! hover:bg-zinc-800! dark:bg-zinc-100! dark:text-zinc-900! dark:hover:bg-white! border-none! font-medium shadow-xs"
            >
              {en ? 'Redeem' : '立即兑换'}
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
};

export default CouponRecords;
