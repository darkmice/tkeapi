/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useState, useEffect, useRef } from 'react';
import { Modal, Button, Typography, Space, Row, Col, QRCode, message, Spin, Result, InputNumber, Grid } from 'antd';
import { useTranslation } from 'react-i18next';
import { WalletOutlined, AlipayCircleOutlined, WechatOutlined, SafetyCertificateOutlined, LockOutlined, CreditCardOutlined, ThunderboltOutlined } from '@ant-design/icons';
import request from '../../utils/request';
import PayNoticeBanner from './PayNoticeBanner';
import { useThemeStore } from '../../store/theme';
import useSettingsStore from '../../store/settings';
import useAuthStore from '../../store/auth';
import {
  getAllinpayMethods,
  getChannelMeta,
  resolveChannelName,
  resolveChannelSubtitle,
  type PaymentChannelUiItem,
  type PaymentMethodId,
} from '../../constants/paymentChannels';

const { Title, Text } = Typography;
const { useBreakpoint } = Grid;

function positiveRechargeLimit(value: unknown, fallback: number): number {
  if (value === undefined || value === null || value === '') return fallback;
  const n = typeof value === 'number' ? value : parseFloat(String(value));
  return Number.isFinite(n) && n > 0 ? n : 0;
}

function resolveRechargeLimits(
  channel: PaymentChannelUiItem | undefined,
  globalMin: number,
  globalMax: number,
): { min: number; max: number } {
  if (channel?.recharge_limit_enabled) {
    return {
      min: positiveRechargeLimit(channel.min_recharge_amount, 0),
      max: positiveRechargeLimit(channel.max_recharge_amount, 0),
    };
  }
  return {
    min: positiveRechargeLimit(globalMin, 0),
    max: positiveRechargeLimit(globalMax, 0),
  };
}

interface RechargeModalProps {
  visible: boolean;
  onCancel: () => void;
  onSuccess: () => void;
  couponOrderNo?: string;
  fixedAmount?: number;
}

/** HyperBC：裸 usdt/usdc→ERC20；USDC@Tron→Tron (USDCOLD)，与官方收银台一致 */
const NETWORK_LABELS: Record<string, string> = {
  trc20: 'Tron (TRC20)',
  usdcold: 'Tron (USDCOLD)',
  erc20: 'Ethereum (ERC20)',
  bep20: 'BNB Smart Chain (BEP20)',
  solana: 'Solana',
  trx: 'Tron',
  eth: 'Ethereum',
  bnb: 'BNB Smart Chain',
  btc: 'Bitcoin',
};
const CURRENCY_ORDER = ['USDT', 'USDC'];

type HyperbcAddr = { coin: string; address: string; amount: string };
type HyperbcCoinDetails = { symbol: string; netKey: string; netKeyUpper: string; network: string };

const hyperbcDetails = (symbol: string, netKey: string): HyperbcCoinDetails => ({
  symbol,
  netKey,
  netKeyUpper: netKey.toUpperCase(),
  network: NETWORK_LABELS[netKey] || (netKey ? netKey.toUpperCase() : ''),
});

const getCoinDetails = (coinStr: string): HyperbcCoinDetails => {
  if (!coinStr) return hyperbcDetails('', '');
  const raw = coinStr.trim().toLowerCase();
  // usdcold → usdc_usdcold，与 usdc_trc20 归一同一路径
  const parts = (raw === 'usdcold' ? 'usdc_usdcold' : raw).split('_').filter(Boolean);
  const symbol = (parts[0] || '').toUpperCase();
  let netKey =
    parts.length >= 2
      ? parts.slice(1).join('_')
      : symbol === 'USDT' || symbol === 'USDC'
        ? 'erc20'
        : parts[0] || '';
  if (symbol === 'USDC' && (netKey === 'trc20' || netKey === 'trx')) netKey = 'usdcold';
  return hyperbcDetails(symbol, netKey);
};

const listHyperbcCurrencies = (addrs: HyperbcAddr[]) =>
  [...new Set(addrs.map((a) => getCoinDetails(a.coin).symbol).filter(Boolean))].sort((a, b) => {
    const ia = CURRENCY_ORDER.indexOf(a);
    const ib = CURRENCY_ORDER.indexOf(b);
    return (ia < 0 ? 99 : ia) - (ib < 0 ? 99 : ib) || a.localeCompare(b);
  });

/** 按币种过滤；顺序保持 addresses 出现序 */
const listHyperbcNetworks = (addrs: HyperbcAddr[], coin: string) => {
  const seen = new Map<string, string>();
  for (const a of addrs) {
    const d = getCoinDetails(a.coin);
    if (d.symbol === coin && d.netKey && !seen.has(d.netKey)) seen.set(d.netKey, d.network);
  }
  return [...seen.entries()].map(([netKey, label]) => ({
    keyUpper: netKey.toUpperCase(),
    label,
  }));
};

const findHyperbcAddress = (addrs: HyperbcAddr[], coin: string, netUpper: string) =>
  addrs.find((a) => {
    const d = getCoinDetails(a.coin);
    return d.symbol === coin && d.netKeyUpper === netUpper;
  }) || null;

const pickDefaultHyperbc = (addrs: HyperbcAddr[]) => {
  if (!addrs.length) return { coin: '', net: '', address: null as HyperbcAddr | null };
  const coin = listHyperbcCurrencies(addrs)[0] || getCoinDetails(addrs[0].coin).symbol;
  const net = listHyperbcNetworks(addrs, coin)[0]?.keyUpper || '';
  return { coin, net, address: findHyperbcAddress(addrs, coin, net) };
};

const RechargeModal: React.FC<RechargeModalProps> = ({ visible, onCancel, onSuccess, couponOrderNo, fixedAmount }) => {
  const { t, i18n } = useTranslation();
  const { user } = useAuthStore();
  const { settings } = useSettingsStore();
  const currencySymbol = settings?.currency?.currency_symbol || '¥';
  const currencyUnit = settings?.currency?.currency_unit || '元';

  const amounts = settings?.currency?.quick_amounts || [20, 50, 100, 500, 1000, 5000];

  const [selectedAmount, setSelectedAmount] = useState<number | null>(null);
  const [customAmount, setCustomAmount] = useState<number | null>(null);
  const [isCustom, setIsCustom] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  useEffect(() => {
    if (amounts.length > 0 && (selectedAmount === null || !amounts.includes(selectedAmount)) && !isCustom) {
      const defaultSel = amounts.includes(50) ? 50 : amounts[0];
      setSelectedAmount(defaultSel);
    }
  }, [amounts, isCustom]);
  const [paymentMethod, setPaymentMethod] = useState<PaymentMethodId>('alipay');
  const [selectedChannel, setSelectedChannel] = useState<string>('alipay');
  const [loading, setLoading] = useState(false);

  const [paymentChannels, setPaymentChannels] = useState<PaymentChannelUiItem[]>([]);
  const [fetchingSettings, setFetchingSettings] = useState(true);

  const [qrCodeUrl, setQrCodeUrl] = useState<string>('');
  const [outTradeNo, setOutTradeNo] = useState<string>('');
  const [payStatus, setPayStatus] = useState<'idle' | 'paying' | 'success'>('idle');
  const [hyperbcData, setHyperbcData] = useState<{ addresses?: HyperbcAddr[] } | null>(null);
  const [selectedAddress, setSelectedAddress] = useState<HyperbcAddr | null>(null);
  
  // HyperBC 界面状态
  const [hyperbcStep, setHyperbcStep] = useState<'select' | 'pay'>('select');
  const [hyperbcNetwork, setHyperbcNetwork] = useState<string>('');
  const [hyperbcCoin, setHyperbcCoin] = useState<string>('');
  const [timeLeft, setTimeLeft] = useState(900);

  const timerRef = useRef<any>(null);

  const finalAmount = couponOrderNo ? Number(fixedAmount || 0) : (isCustom ? (customAmount || 0) : (selectedAmount || 0));

  // 渠道不收默认货币时按公开汇率预估应付金额（向上取整，与后端下单口径一致；以下单返回为准）
  const defaultCurrencyCode = String(settings?.currency?.default_currency || '').toUpperCase();
  const selectedPayChannel = paymentChannels.find((c) => c.id === selectedChannel);
  const payCurrency = selectedPayChannel?.pay_currency;
  const payDecimals = selectedPayChannel?.pay_decimals ?? 2;
  const payRate = selectedPayChannel?.pay_rate;
  const needsConversion = !!payCurrency && payCurrency !== defaultCurrencyCode && !!payRate;
  const payScale = 10 ** payDecimals;
  const payPreview = needsConversion
    ? Math.ceil(finalAmount * (payRate as number) * payScale - 1e-6) / payScale
    : finalAmount;
  const paySymbol = selectedPayChannel?.pay_currency_symbol || payCurrency || '';
  const [orderPay, setOrderPay] = useState<{ currency: string; amount: number } | null>(null);
  const paidLabel = orderPay && orderPay.currency !== defaultCurrencyCode
    ? `${paySymbol} ${orderPay.amount.toFixed(payDecimals)}`
    : `${currencySymbol} ${finalAmount.toFixed(2)}`;

  useEffect(() => {
    if (visible) {
      fetchPaymentSettings();
      resetState();
    } else {
      clearTimer();
    }
    return () => clearTimer();
  }, [visible]);

  const resetState = () => {
    clearTimer();
    setQrCodeUrl('');
    setOutTradeNo('');
    setOrderPay(null);
    setPayStatus('idle');
    setHyperbcData(null);
    setSelectedAddress(null);
    setHyperbcStep('select');
    setHyperbcNetwork('');
    setHyperbcCoin('');
    setTimeLeft(900);
    const defaultSel = amounts.includes(50) ? 50 : (amounts[0] || null);
    setSelectedAmount(defaultSel);
    setCustomAmount(null);
    setIsCustom(false);
    setErrorMessage(null);
  };

  const clearTimer = () => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
  };

  const hyperbcAddresses: HyperbcAddr[] = hyperbcData?.addresses || [];
  const hyperbcCurrencies = listHyperbcCurrencies(hyperbcAddresses);
  const hyperbcNetworksForCoin = listHyperbcNetworks(hyperbcAddresses, hyperbcCoin);

  const handleCoinChange = (coin: string) => {
    setHyperbcCoin(coin);
    const nets = listHyperbcNetworks(hyperbcAddresses, coin).map((n) => n.keyUpper);
    const nextNet = nets.includes(hyperbcNetwork) ? hyperbcNetwork : (nets[0] || '');
    setHyperbcNetwork(nextNet);
    setSelectedAddress(findHyperbcAddress(hyperbcAddresses, coin, nextNet));
  };

  const handleNetworkChange = (net: string) => {
    setHyperbcNetwork(net);
    setSelectedAddress(findHyperbcAddress(hyperbcAddresses, hyperbcCoin, net));
  };

  // 倒计时控制
  useEffect(() => {
    let interval: any = null;
    if (payStatus === 'paying' && paymentMethod === 'hyperbc' && hyperbcStep === 'pay') {
      setTimeLeft(900);
      interval = setInterval(() => {
        setTimeLeft((prev) => {
          if (prev <= 1) {
            clearInterval(interval);
            return 0;
          }
          return prev - 1;
        });
      }, 1000);
    }
    return () => {
      if (interval) clearInterval(interval);
    };
  }, [payStatus, paymentMethod, hyperbcStep]);

  const formatTimeLeft = (seconds: number) => {
    const m = Math.floor(seconds / 60).toString().padStart(2, '0');
    const s = (seconds % 60).toString().padStart(2, '0');
    return `${m}:${s}`;
  };

  const applyChannelSelection = (channel: PaymentChannelUiItem) => {
    setErrorMessage(null);
    setSelectedChannel(channel.id);
    if (channel.id === 'allinpay') {
      const methods = getAllinpayMethods(channel);
      if (methods.length >= 1) {
        setPaymentMethod(methods[0]);
      }
      return;
    }
    setPaymentMethod(channel.id as PaymentMethodId);
  };

  const fetchPaymentSettings = async () => {
    setFetchingSettings(true);
    try {
      const res = await (request.get('/settings') as any);
      const channels = Array.isArray(res?.payment_channels) ? res.payment_channels : [];
      // 兼容旧公开接口：仅有 payment.*_enabled 时回退
      const payment = res?.payment;
      const fallback: PaymentChannelUiItem[] = [
        { id: 'alipay', enabled: !!payment?.alipay_enabled, sort_order: 70 },
        { id: 'wechat', enabled: !!payment?.wechat_enabled, sort_order: 60 },
        {
          id: 'allinpay',
          enabled: !!payment?.allinpay_enabled,
          sort_order: 50,
          allinpay_methods: payment?.allinpay_enabled
            ? ['allinpay_wechat', 'allinpay_alipay']
            : [],
        },
        { id: 'stripe', enabled: !!payment?.stripe_enabled, sort_order: 30 },
        { id: 'hyperbc', enabled: !!payment?.hyperbc_enabled, sort_order: 10 },
      ];
      const list = (channels.length ? channels : fallback)
        .filter((c: any) => c && c.id)
        // 旧公开数据若仍拆成两个通联渠道，合并展示
        .reduce((acc: PaymentChannelUiItem[], c: any) => {
          if (c.id === 'allinpay_wechat' || c.id === 'allinpay_alipay') {
            let ap = acc.find((x) => x.id === 'allinpay');
            if (!ap) {
              ap = {
                id: 'allinpay',
                enabled: false,
                sort_order: c.sort_order || 50,
                allinpay_methods: [],
              };
              acc.push(ap);
            }
            if (c.enabled) {
              ap.enabled = true;
              const methods = ap.allinpay_methods || [];
              if (!methods.includes(c.id)) methods.push(c.id);
              ap.allinpay_methods = methods;
              ap.sort_order = Math.max(ap.sort_order || 0, c.sort_order || 0);
            }
            return acc;
          }
          acc.push(c);
          return acc;
        }, [])
        .sort((a: any, b: any) => (b.sort_order || 0) - (a.sort_order || 0) || String(a.id).localeCompare(String(b.id)));
      setPaymentChannels(list);
      const firstEnabled = list.find((c: any) => c.enabled);
      if (firstEnabled) {
        applyChannelSelection(firstEnabled);
      }
    } catch (e) {
      console.error(e);
    } finally {
      setFetchingSettings(false);
    }
  };

  const startPolling = (tradeNo: string) => {
    clearTimer();
    timerRef.current = setInterval(async () => {
      try {
        const res = await (request.get(`/finance/pay/status/${tradeNo}`) as any);
        if (res?.status === 'paid') {
          clearTimer();
          setPayStatus('success');
          message.success(couponOrderNo ? (i18n.language?.toLowerCase().startsWith('en') ? 'Paid. The coupon is in your account.' : '支付成功，卡券已发放') : t('recharge.recharge_success', '充值成功！'));
          setTimeout(() => { onSuccess(); }, 2000);
        }
      } catch (err) {
        console.error('Polling error', err);
      }
    }, 3000);
  };

  const globalMinRecharge = settings?.currency?.min_recharge_amount !== undefined
    ? parseFloat(String(settings.currency.min_recharge_amount))
    : 5;
  const globalMaxRecharge = settings?.currency?.max_recharge_amount !== undefined
    ? parseFloat(String(settings.currency.max_recharge_amount))
    : 10000;
  const rechargeLimits = resolveRechargeLimits(
    paymentChannels.find((c) => c.id === selectedChannel),
    globalMinRecharge,
    globalMaxRecharge,
  );

  const handleCreateOrder = async () => {
    const minRechargeLimit = rechargeLimits.min;
    const maxRechargeLimit = rechargeLimits.max;

    if (selectedChannel === 'allinpay') {
      const ap = paymentChannels.find((c) => c.id === 'allinpay');
      const methods = ap ? getAllinpayMethods(ap) : [];
      if (!methods.includes(paymentMethod)) {
        setErrorMessage(methods.length > 1
          ? t('recharge.allinpay_pick_error', '请选择通联支付方式（微信或支付宝）')
          : t('recharge.allinpay_unavailable', '通联支付暂不可用'));
        return;
      }
    }

    if (!couponOrderNo && minRechargeLimit > 0 && finalAmount < minRechargeLimit) {
      setErrorMessage(t('recharge.min_amount_error', { 
        defaultValue: `充值金额不能小于 ${minRechargeLimit} ${currencyUnit}`, 
        unit: currencyUnit, 
        limit: minRechargeLimit 
      }));
      return;
    }
    if (!couponOrderNo && maxRechargeLimit > 0 && finalAmount > maxRechargeLimit) {
      setErrorMessage(t('recharge.max_amount_error', {
        defaultValue: `充值金额不能大于 ${maxRechargeLimit} ${currencyUnit}`,
        unit: currencyUnit,
        limit: maxRechargeLimit
      }));
      return;
    }
    if (finalAmount < 0.01) {
      setErrorMessage(t('recharge.min_amount_error', { 
        defaultValue: `充值金额不能小于 0.01 ${currencyUnit}`, 
        unit: currencyUnit, 
        limit: 0.01 
      }));
      return;
    }
    if (paymentMethod === 'stripe' && (payCurrency || currencyUnit).toUpperCase() === 'USD' && payPreview < 0.5) {
      setErrorMessage(t('recharge.stripe_min_amount_error', {
        defaultValue: 'Stripe 支付单笔金额不能小于 0.50 USD',
      }));
      return;
    }
    setErrorMessage(null);
    clearTimer();
    setLoading(true);
    const isMobile = /Android|webOS|iPhone|iPad|IEMobile|Opera Mini/i.test(navigator.userAgent);
    try {
      const reqBody: any = {
        amount: finalAmount,
        payment_method: paymentMethod,
        is_mobile: isMobile,
        coupon_order_no: couponOrderNo || undefined,
      };
      const res = await (request.post('/finance/pay/create', reqBody, { skipErrorHandler: true } as any) as any);
      
      setOutTradeNo(res.out_trade_no);
      if (res.pay_currency && typeof res.pay_amount === 'number') {
        setOrderPay({ currency: String(res.pay_currency).toUpperCase(), amount: res.pay_amount });
      }
      setPayStatus('paying');
      
      if (paymentMethod === 'alipay') {
        window.location.href = res.payment_url;
      } else if (paymentMethod === 'stripe') {
        window.open(res.payment_url, '_blank');
        startPolling(res.out_trade_no);
      } else if (paymentMethod === 'wechat') {
        setQrCodeUrl(res.payment_url);
        startPolling(res.out_trade_no);
      } else if (paymentMethod === 'allinpay_wechat' || paymentMethod === 'allinpay_alipay') {
        if (isMobile) {
          window.location.href = res.payment_url;
        } else {
          setQrCodeUrl(res.payment_url);
          startPolling(res.out_trade_no);
        }
      } else if (paymentMethod === 'hyperbc') {
        // HyperBC: 不再跳转收银台，而是记录返回的地址列表，展示在弹窗中并启动轮询
        if (res.hyperbc_data) {
          setHyperbcData(res.hyperbc_data);
          const picked = pickDefaultHyperbc(res.hyperbc_data.addresses || []);
          setHyperbcCoin(picked.coin);
          setHyperbcNetwork(picked.net);
          setSelectedAddress(picked.address);
        }
        setHyperbcStep('select');
        startPolling(res.out_trade_no);
      }
    } catch (err: any) {
      let rawMsg = err.response?.data?.error?.message || err.response?.data?.error || err.message || t('recharge.pay_info_fail', '获取支付信息失败');
      let errMsgStr = typeof rawMsg === 'object' ? JSON.stringify(rawMsg) : String(rawMsg);
      if (errMsgStr.includes('<html') || errMsgStr.includes('<!DOCTYPE') || errMsgStr.includes('<body')) {
        errMsgStr = t('recharge.gateway_error', '支付网关响应异常，请稍后重试');
      } else if (errMsgStr.startsWith('{') && errMsgStr.endsWith('}')) {
        try {
          const parsed = JSON.parse(errMsgStr);
          errMsgStr = parsed?.error?.message || parsed?.message || errMsgStr;
        } catch (_) {}
      }
      if (
        errMsgStr.includes('充值金额不能小于') ||
        errMsgStr.includes('金额必须大于') ||
        errMsgStr.includes('min_amount_error') ||
        errMsgStr.includes('金额不能小于')
      ) {
        setErrorMessage(errMsgStr);
      } else {
        message.error(errMsgStr);
      }
    } finally {
      setLoading(false);
    }
  };

  const handlePresetClick = (amt: number) => {
    setSelectedAmount(amt);
    setIsCustom(false);
    setCustomAmount(null);
    setErrorMessage(null);
  };

  const handleCustomFocus = () => {
    setIsCustom(true);
    setSelectedAmount(null);
    setErrorMessage(null);
  };

  const { themeMode } = useThemeStore();
  const isLight = themeMode === 'light';
  const screens = useBreakpoint();
  const isMobile = !screens.md;

  const modalStyles = {
    content: {
      background: isLight ? '#ffffff' : '#09090b',
      border: isLight ? '1px solid #e4e4e7' : '1px solid #27272a',
      borderRadius: isMobile ? 16 : 20,
      padding: 0,
      boxShadow: isLight
        ? '0 20px 45px -10px rgba(0, 0, 0, 0.12)'
        : '0 25px 50px -12px rgba(0, 0, 0, 0.75)',
      overflow: 'hidden',
      maxWidth: isMobile ? 'calc(100vw - 24px)' : undefined,
    },
    body: { padding: isMobile ? '12px 14px 16px' : '14px 28px 24px' },
    header: { display: 'none' as const },
    mask: { backgroundColor: 'rgba(0, 0, 0, 0.65)', backdropFilter: 'blur(4px)' },
  };

  const borderIdle = isLight ? '#e4e4e7' : '#27272a';
  const bgIdle = isLight ? '#fcfcfd' : '#141417';
  const labelColor = isLight ? '#09090b' : '#f4f4f5';
  const descColor = isLight ? '#52525b' : '#a1a1aa';
  const subColor = isLight ? '#71717a' : '#a1a1aa';
  const summaryBg = isLight ? '#f4f4f5' : '#18181b';
  const summaryBorder = isLight ? '#e4e4e7' : '#27272a';
  const titleColor = isLight ? '#09090b' : '#ffffff';
  const accent = isLight ? '#18181b' : '#fafafa';

  if (fetchingSettings) {
    return (
      <Modal open={visible} footer={null} closable={false} centered styles={modalStyles}>
        <div style={{ textAlign: 'center', padding: '50px 0' }}><Spin size="large" /></div>
      </Modal>
    );
  }

  if (!paymentChannels.some((c) => c.enabled)) {
    return (
      <Modal open={visible} footer={null} onCancel={onCancel} centered styles={modalStyles}>
        <Result
          status="warning"
          title={t('recharge.not_available', '在线充值暂不可用')}
          subTitle={t('recharge.not_available_desc', '管理员尚未开启或正确配置在线支付功能')}
        />
      </Modal>
    );
  }

  const defaultIcon = (id: string) => {
    const color = getChannelMeta(id)?.accent || '#666';
    if (id === 'alipay' || id === 'allinpay_alipay') return <AlipayCircleOutlined style={{ fontSize: 22, color }} />;
    if (id === 'wechat' || id === 'allinpay_wechat') return <WechatOutlined style={{ fontSize: 22, color }} />;
    if (id === 'allinpay') return <ThunderboltOutlined style={{ fontSize: 22, color }} />;
    if (id === 'stripe') return <CreditCardOutlined style={{ fontSize: 22, color }} />;
    if (id === 'hyperbc') return <span style={{ fontSize: 20, fontWeight: 'bold', color, display: 'inline-block', lineHeight: 1 }}>₿</span>;
    return <WalletOutlined style={{ fontSize: 22, color }} />;
  };

  // 多支付渠道：按后台排序与展示配置渲染（通联聚合为一项）
  const paymentOptions = paymentChannels
    .filter((c) => c.enabled)
    .map((c) => {
      const item = c as PaymentChannelUiItem;
      const meta = getChannelMeta(c.id);
      const accent = meta?.accent || '#1677ff';
      const logoUrl = (c.logo_url || '').trim();
      return {
        key: c.id,
        channel: item,
        enabled: true,
        name: resolveChannelName(item, i18n.language),
        badge: resolveChannelSubtitle(item, i18n.language),
        badgeBg: isLight ? `${accent}1a` : `${accent}33`,
        badgeColor: accent,
        icon: logoUrl
          ? <img src={logoUrl} alt="" style={{ width: 22, height: 22, objectFit: 'contain', borderRadius: 4 }} />
          : defaultIcon(c.id),
        activeBorderColor: accent,
        activeBg: isLight ? `${accent}0f` : `${accent}26`,
      };
    });

  const selectedAllinpay = paymentChannels.find((c) => c.id === 'allinpay' && c.enabled);
  const allinpayMethods = selectedChannel === 'allinpay' && selectedAllinpay
    ? getAllinpayMethods(selectedAllinpay)
    : [];
  const showAllinpaySubPicker = allinpayMethods.length > 1;

  const getPayButtonBackground = (method: string) => {
    switch (method) {
      case 'alipay':
      case 'allinpay_alipay':
        return 'linear-gradient(135deg, #1677ff, #0958d9)';
      case 'wechat':
      case 'allinpay_wechat':
        return 'linear-gradient(135deg, #07c160, #059669)';
      case 'stripe':
        return 'linear-gradient(135deg, #635bff, #4b45c6)';
      case 'hyperbc':
        return 'linear-gradient(135deg, #8b5cf6, #6d28d9)';
      default:
        return isLight ? '#18181b' : '#fafafa';
    }
  };

  const getPayButtonShadow = (method: string) => {
    switch (method) {
      case 'alipay':
      case 'allinpay_alipay':
        return '0 4px 16px rgba(22, 119, 255, 0.35)';
      case 'wechat':
      case 'allinpay_wechat':
        return '0 4px 16px rgba(7, 193, 96, 0.35)';
      case 'stripe':
        return '0 4px 16px rgba(99, 91, 255, 0.35)';
      case 'hyperbc':
        return '0 4px 16px rgba(139, 92, 246, 0.35)';
      default:
        return isLight ? '0 4px 16px rgba(24, 24, 27, 0.2)' : '0 4px 16px rgba(0, 0, 0, 0.4)';
    }
  };

  return (
    <Modal
      open={visible}
      destroyOnClose
      width={isMobile ? '100%' : 840}
      title={null}
      footer={null}
      onCancel={onCancel}
      styles={modalStyles}
      centered
    >
      <style>{`
        .recharge-pay-btn.ant-btn,
        .recharge-pay-btn.ant-btn:hover,
        .recharge-pay-btn.ant-btn:focus,
        .recharge-pay-btn.ant-btn > span,
        .recharge-pay-btn .anticon {
          color: #ffffff !important;
        }
      `}</style>
      {/* 顶栏 Header */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        borderBottom: `1px solid ${borderIdle}`,
        paddingBottom: isMobile ? 10 : 16,
        marginBottom: isMobile ? 12 : 28,
        paddingRight: isMobile ? 28 : 36,
        gap: 8,
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: isMobile ? 8 : 12, minWidth: 0, flex: 1 }}>
          <div style={{
            width: isMobile ? 36 : 44,
            height: isMobile ? 36 : 44,
            borderRadius: isMobile ? 10 : 12,
            background: isLight ? '#18181b' : '#fafafa',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            color: isLight ? '#ffffff' : '#18181b',
            boxShadow: isLight ? '0 4px 12px rgba(24, 24, 27, 0.15)' : '0 4px 12px rgba(0, 0, 0, 0.4)',
            flexShrink: 0,
          }}>
            <WalletOutlined style={{ fontSize: isMobile ? 18 : 22 }} />
          </div>
          <div style={{ minWidth: 0 }}>
            <Title level={4} style={{ margin: 0, color: titleColor, fontSize: isMobile ? 15 : 18, fontWeight: 700, lineHeight: 1.2 }}>
              {couponOrderNo ? (i18n.language?.toLowerCase().startsWith('en') ? 'Pay for coupon' : '支付卡券订单') : t('recharge.title', '钱包余额充值')}
            </Title>
            {!isMobile && (
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 2 }}>
                <LockOutlined style={{ fontSize: 11, color: '#10b981' }} />
                <Text style={{ fontSize: 12, color: subColor }}>{t('recharge.secure_channel', '256-bit 安全加密支付通道')}</Text>
              </div>
            )}
          </div>
        </div>

        <div style={{
          display: 'flex',
          alignItems: 'center',
          gap: 4,
          background: summaryBg,
          padding: isMobile ? '4px 8px' : '6px 14px',
          borderRadius: 10,
          border: `1px solid ${summaryBorder}`,
          flexShrink: 0,
          maxWidth: isMobile ? '42%' : undefined,
        }}>
          <Text style={{ fontSize: isMobile ? 10 : 12, color: subColor, whiteSpace: 'nowrap' }}>UID</Text>
          <Text strong style={{
            fontSize: isMobile ? 11 : 13,
            color: labelColor,
            fontFamily: 'monospace',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}>
            {user?.uid || user?.id || 'User'}
          </Text>
        </div>
      </div>

      {payStatus === 'success' ? (
        <Result
          status="success"
          title={t('recharge.success_title', '支付成功！')}
          subTitle={t('recharge.success_subtitle', '您的钱包余额已经更新')}
          extra={[
            <Button type="primary" key="done" onClick={onSuccess} style={{ borderRadius: 10, height: 42, paddingLeft: 24, paddingRight: 24 }}>
              {t('recharge.done', '完成')}
            </Button>
          ]}
        />
      ) : payStatus === 'paying' && (paymentMethod === 'wechat' || paymentMethod === 'allinpay_wechat' || paymentMethod === 'allinpay_alipay') ? (
        <div style={{ textAlign: 'center', padding: '10px 0' }}>
          <Text type="secondary" style={{ display: 'block', marginBottom: 16, fontSize: 14 }}>
            {paymentMethod === 'allinpay_alipay' 
              ? t('recharge.alipay_scan', '请使用支付宝扫一扫支付') 
              : t('recharge.wechat_scan', '请使用微信扫一扫支付')}
          </Text>
          <div style={{ padding: 16, background: '#fff', borderRadius: 16, display: 'inline-block', boxShadow: '0 8px 30px rgba(0,0,0,0.15)' }}>
            <QRCode value={qrCodeUrl} size={200} color="#000000" />
          </div>
          <div style={{ marginTop: 20 }}>
            <Title level={3} style={{ color: '#ef4444', margin: 0, fontWeight: 800 }}>{paidLabel}</Title>
            <Text type="secondary" style={{ fontSize: 13 }}>{t('recharge.order_no', '订单号: ')}{outTradeNo}</Text>
          </div>
          <Button style={{ marginTop: 20, borderRadius: 10, height: 40 }} onClick={resetState}>{t('recharge.return_modify', '返回修改')}</Button>
        </div>
      ) : payStatus === 'paying' && paymentMethod === 'stripe' ? (
        <div style={{ textAlign: 'center', padding: '10px 0' }}>
          <div style={{
            display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
            width: 64, height: 64, borderRadius: 16,
            background: 'linear-gradient(135deg, #635bff, #4b45c6)',
            marginBottom: 20, boxShadow: '0 8px 24px rgba(99, 91, 255, 0.3)',
          }}>
            <CreditCardOutlined style={{ fontSize: 32, color: '#fff' }} />
          </div>
          <Title level={4} style={{ color: titleColor, margin: '0 0 8px 0' }}>{t('recharge.stripe_waiting', '等待 Stripe 支付完成')}</Title>
          <Text type="secondary" style={{ display: 'block', marginBottom: 20 }}>
            {t('recharge.stripe_desc', '请在新打开的页面完成支付，支付成功后此页面将自动更新')}
          </Text>
          <Spin size="large" />
          <div style={{ marginTop: 20 }}>
            <Title level={3} style={{ color: '#ef4444', margin: 0, fontWeight: 800 }}>{paidLabel}</Title>
            <Text type="secondary" style={{ fontSize: 13 }}>{t('recharge.order_no', '订单号: ')}{outTradeNo}</Text>
          </div>
          <Button style={{ marginTop: 20, borderRadius: 10, height: 40 }} onClick={resetState}>{t('recharge.return_modify', '返回修改')}</Button>
        </div>
      ) : payStatus === 'paying' && paymentMethod === 'hyperbc' ? (
        hyperbcStep === 'select' ? (
          <div style={{ textAlign: 'left', maxWidth: 520, margin: '0 auto' }}>
            <Title level={4} style={{ color: titleColor, textAlign: 'center', marginBottom: 16, fontWeight: 600 }}>
              {t('recharge.hyperbc_account', 'HyperBC 账户充值')}
            </Title>
            <Text type="secondary" style={{ display: 'block', textAlign: 'center', marginBottom: 20, fontSize: 13 }}>
              {t('recharge.hyperbc_select_pay', '请选择支付币种与网络')}
            </Text>

            {/* 1. 币种 */}
            <div style={{ marginBottom: 16 }}>
              <Text style={{ color: descColor, fontSize: 13, display: 'block', marginBottom: 8 }}>{t('recharge.select_pay_currency', '选择支付币种')}</Text>
              <div style={{ display: 'flex', gap: 10 }}>
                {hyperbcCurrencies.map((coin) => {
                  const isSel = hyperbcCoin === coin;
                  const accentColor = coin === 'USDC' ? '#2775CA' : '#26A17B';
                  return (
                    <div
                      key={coin}
                      onClick={() => handleCoinChange(coin)}
                      style={{
                        flex: 1,
                        height: 52,
                        borderRadius: 12,
                        cursor: 'pointer',
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        gap: 8,
                        border: `2px solid ${isSel ? '#8b5cf6' : borderIdle}`,
                        background: isSel ? 'rgba(139, 92, 246, 0.12)' : bgIdle,
                        transition: 'all 0.2s ease',
                      }}
                    >
                      <span style={{
                        width: 22, height: 22, borderRadius: '50%', background: accentColor,
                        color: '#fff', fontSize: 11, fontWeight: 700,
                        display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
                      }}>
                        {coin === 'USDC' ? '$' : '₮'}
                      </span>
                      <Text strong style={{ color: isSel ? '#8b5cf6' : labelColor, fontSize: 16 }}>{coin}</Text>
                    </div>
                  );
                })}
              </div>
            </div>

            {/* 2. 网络 */}
            <div style={{ marginBottom: 20 }}>
              <Text style={{ color: descColor, fontSize: 13, display: 'block', marginBottom: 8 }}>{t('recharge.select_network', '选择网络')}</Text>
              <div style={{
                border: `1px solid ${borderIdle}`,
                borderRadius: 12,
                padding: 12,
                display: 'flex',
                gap: 8,
                flexWrap: 'wrap',
              }}>
                {hyperbcNetworksForCoin.length === 0 ? (
                  <Text type="secondary" style={{ fontSize: 12 }}>{t('recharge.no_network', '暂无可用网络')}</Text>
                ) : hyperbcNetworksForCoin.map((net) => {
                  const isSel = hyperbcNetwork === net.keyUpper;
                  return (
                    <Button
                      key={net.keyUpper}
                      type={isSel ? 'primary' : 'default'}
                      onClick={() => handleNetworkChange(net.keyUpper)}
                      style={{
                        height: 40,
                        borderRadius: 10,
                        fontWeight: isSel ? 600 : 400,
                        borderColor: isSel ? '#8b5cf6' : borderIdle,
                        backgroundColor: isSel ? '#8b5cf6' : bgIdle,
                        color: isSel ? '#fff' : labelColor,
                      }}
                    >
                      {net.label}
                    </Button>
                  );
                })}
              </div>
            </div>

            {/* 金额显示 */}
            <div style={{ marginBottom: 20 }}>
              <Text style={{ color: descColor, fontSize: 13, display: 'block', marginBottom: 8 }}>{t('recharge.payable_amount', '应付金额')}</Text>
              <div style={{
                background: bgIdle,
                border: `1px solid ${borderIdle}`,
                borderRadius: 12,
                padding: '14px 16px',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between'
              }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <span style={{ color: subColor, fontSize: 16 }}>$</span>
                  <Text style={{ color: labelColor, fontSize: 20, fontWeight: 'bold' }}>
                    {selectedAddress ? parseFloat(selectedAddress.amount).toFixed(2) : finalAmount.toFixed(2)}
                  </Text>
                </div>
                <Text style={{ color: subColor, fontWeight: 500, fontSize: 15 }}>{hyperbcCoin}</Text>
              </div>
            </div>

            <Button
              type="primary"
              block
              disabled={!selectedAddress || !hyperbcCoin || !hyperbcNetwork}
              onClick={() => setHyperbcStep('pay')}
              style={{
                height: 48,
                borderRadius: 12,
                background: 'linear-gradient(135deg, #8b5cf6, #6d28d9)',
                border: 'none',
                fontWeight: 600,
                fontSize: 16,
                boxShadow: '0 4px 16px rgba(139, 92, 246, 0.35)',
              }}
            >
              {t('recharge.go_pay_now', '前往支付 ➔')}
            </Button>

            <div style={{ marginTop: 12, textAlign: 'center' }}>
              <Button type="text" size="small" onClick={resetState} style={{ color: subColor }}>
                {t('recharge.return_modify_amount', '返回修改充值金额')}
              </Button>
            </div>
          </div>
        ) : (
          <div style={{ textAlign: 'center', maxWidth: 560, margin: '0 auto' }}>
            {selectedAddress && (() => {
              const details = getCoinDetails(selectedAddress.coin);
              const amtStr = selectedAddress.amount || '0.00';
              const dotIdx = amtStr.indexOf('.');
              const integerPart = dotIdx !== -1 ? amtStr.substring(0, dotIdx) : amtStr;
              const decimalPart = dotIdx !== -1 ? amtStr.substring(dotIdx) : '';

              return (
                <div>
                  <Text type="secondary" style={{ fontSize: 13, display: 'block', marginBottom: 4 }}>
                    {t('recharge.amount_due', '待支付总额')}
                  </Text>
                  <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'center', marginBottom: 4 }}>
                    <span style={{ fontSize: 32, fontWeight: 800, color: labelColor }}>{integerPart}</span>
                    <span style={{ fontSize: 22, fontWeight: 700, color: '#ef4444' }}>{decimalPart}</span>
                    <span style={{ fontSize: 16, fontWeight: 600, color: subColor, marginLeft: 6 }}>{details.symbol}</span>
                  </div>
                  <div style={{ color: '#ef4444', fontSize: 12, fontWeight: 500, marginBottom: 16 }}>
                    ⚠️ {t('recharge.pay_exact_network', '请通过此网络支付精确金额：')}<span style={{ textDecoration: 'underline' }}>{details.network}</span>
                  </div>

                  <div style={{
                    padding: 14,
                    background: '#fff',
                    borderRadius: 14,
                    display: 'inline-block',
                    boxShadow: '0 4px 20px rgba(0,0,0,0.12)',
                    marginBottom: 16
                  }}>
                    <QRCode value={selectedAddress.address} size={160} color="#000000" bordered={false} />
                  </div>

                  {/* 收款地址卡片 */}
                  <div style={{ textAlign: 'left', marginBottom: 16 }}>
                    <Text type="secondary" style={{ fontSize: 12, display: 'block', marginBottom: 4 }}>
                      {t('recharge.receive_address', { defaultValue: '收款地址 ({{network}})', network: details.network })}
                    </Text>
                    <Typography.Paragraph
                      copyable={{ text: selectedAddress.address }}
                      style={{
                        color: labelColor,
                        fontSize: 13,
                        background: summaryBg,
                        padding: '10px 12px',
                        borderRadius: 10,
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'space-between',
                        border: `1px solid ${summaryBorder}`,
                        margin: 0,
                      }}
                    >
                      <span style={{ fontFamily: 'monospace', wordBreak: 'break-all', paddingRight: 8 }}>
                        {selectedAddress.address}
                      </span>
                    </Typography.Paragraph>
                  </div>

                  <div style={{
                    background: isLight ? 'rgba(250, 173, 20, 0.08)' : 'rgba(250, 173, 20, 0.12)',
                    border: '1px solid rgba(250, 173, 20, 0.3)',
                    borderRadius: 10,
                    padding: '10px 12px',
                    textAlign: 'left',
                    marginBottom: 16,
                    color: isLight ? '#d48806' : '#faad14',
                    fontSize: 12,
                    lineHeight: 1.5
                  }}>
                    ⚠️ {t('recharge.exchange_withdraw_tip', '交易所提币提示：如使用 币安、OKX 等交易所提币，请手动加上提现手续费，确保到账金额完全一致。')}
                  </div>

                  <div style={{ marginBottom: 16 }}>
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4 }}>
                      <Text type="secondary" style={{ fontSize: 12 }}>{t('recharge.waiting_payment', '等待支付中')}</Text>
                      <Text style={{ fontSize: 12, fontFamily: 'monospace', fontWeight: 'bold', color: labelColor }}>
                        {formatTimeLeft(timeLeft)}
                      </Text>
                    </div>
                    <div style={{
                      width: '100%',
                      height: 5,
                      background: borderIdle,
                      borderRadius: 3,
                      overflow: 'hidden'
                    }}>
                      <div style={{
                        width: `${(timeLeft / 900) * 100}%`,
                        height: '100%',
                        background: '#3b82f6',
                        transition: 'width 1s linear'
                      }} />
                    </div>
                  </div>

                  <div style={{ display: 'flex', justifyContent: 'center', gap: 12 }}>
                    <Button style={{ borderRadius: 10 }} onClick={() => setHyperbcStep('select')}>
                      {t('recharge.change_coin_network', '修改币种网络')}
                    </Button>
                    <Button style={{ borderRadius: 10 }} onClick={resetState}>
                      {t('recharge.change_amount', '修改充值金额')}
                    </Button>
                  </div>
                </div>
              );
            })()}
          </div>
        )
      ) : (
        /* 主选择界面：横版双列 (Left: 金额选择，Right: 支付方式) */
        <>
        <PayNoticeBanner />
        <Row gutter={isMobile ? [0, 12] : [24, 20]} style={{ margin: 0 }}>
          {/* 左侧 Column：选择支付金额 */}
          <Col
            xs={24}
            md={11}
            style={{
              padding: isMobile ? 0 : '0 12px 0 0',
              display: 'flex',
              flexDirection: 'column',
              gap: isMobile ? 10 : 16,
            }}
          >
                {!couponOrderNo && (<>
                <div>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: isMobile ? 8 : 10 }}>
                    <Text strong style={{ fontSize: isMobile ? 13 : 14, color: labelColor }}>
                      {t('recharge.select_amount', { defaultValue: `选择充值金额 (${currencyUnit})`, unit: currencyUnit })}
                    </Text>
                    <Text style={{ fontSize: 11, color: subColor }}>{t('recharge.quick_select', '点击快捷选择')}</Text>
                  </div>
                  
                  {/* 快捷金额卡片网格 */}
                  <Row gutter={[8, 8]}>
                    {amounts.map((amt: number) => {
                      const isSelected = !isCustom && selectedAmount === amt;
                      return (
                        <Col span={8} key={amt}>
                          <div
                            onClick={() => handlePresetClick(amt)}
                            style={{
                              border: `2px solid ${isSelected ? (isLight ? '#18181b' : '#3b82f6') : borderIdle}`,
                              borderRadius: isMobile ? 10 : 12,
                              padding: isMobile ? '8px 0' : '12px 0',
                              textAlign: 'center',
                              cursor: 'pointer',
                              background: isSelected
                                ? (isLight ? '#18181b' : 'rgba(59, 130, 246, 0.15)')
                                : bgIdle,
                              transition: 'all 0.2s cubic-bezier(0.4, 0, 0.2, 1)',
                              boxShadow: isSelected ? (isLight ? '0 4px 14px rgba(24, 24, 27, 0.12)' : '0 4px 14px rgba(59, 130, 246, 0.25)') : 'none',
                            }}
                          >
                            <div style={{
                              fontSize: isMobile ? 15 : 18,
                              fontWeight: 700,
                              color: isSelected ? (isLight ? '#ffffff' : '#60a5fa') : labelColor,
                              lineHeight: 1.2
                            }}>
                              {amt}
                            </div>
                          </div>
                        </Col>
                      );
                    })}
                  </Row>
                </div>

                {/* 自定义金额输入卡片 */}
                <div>
                  <Text strong style={{ fontSize: isMobile ? 12 : 13, color: labelColor, display: 'block', marginBottom: isMobile ? 6 : 8 }}>
                    {t('recharge.custom_amount', '或输入自定义金额')}
                  </Text>
                  <div
                    onClick={handleCustomFocus}
                    style={{
                      border: `2px solid ${isCustom ? (isLight ? '#18181b' : '#3b82f6') : borderIdle}`,
                      borderRadius: isMobile ? 10 : 12,
                      padding: isMobile ? '6px 12px' : '8px 14px',
                      background: isCustom ? (isLight ? 'rgba(24, 24, 27, 0.04)' : 'rgba(59, 130, 246, 0.1)') : bgIdle,
                      transition: 'all 0.2s ease',
                      display: 'flex',
                      alignItems: 'center',
                      gap: 8,
                    }}
                  >
                    <Text strong style={{ color: isCustom ? (isLight ? '#18181b' : '#60a5fa') : descColor, fontSize: 16 }}>{currencySymbol}</Text>
                    <InputNumber
                      min={0.01}
                      max={rechargeLimits.max > 0 ? rechargeLimits.max : 50000}
                      precision={2}
                      placeholder={t('recharge.input_amount', '输入金额')}
                      value={customAmount}
                      onChange={(val) => { setCustomAmount(val); setIsCustom(true); setSelectedAmount(null); setErrorMessage(null); }}
                      onFocus={handleCustomFocus}
                      controls={false}
                      variant="borderless"
                      style={{ flex: 1, background: 'transparent', fontSize: 15 }}
                    />
                    <Text style={{ color: subColor, fontSize: 13 }}>{currencyUnit}</Text>
                  </div>
                </div>
                </>)}

                {/* 应付金额汇总卡片 */}
                <div style={{
                  marginTop: isMobile ? 0 : 'auto',
                  padding: isMobile ? '10px 12px' : '14px 18px',
                  background: summaryBg,
                  borderRadius: isMobile ? 10 : 14,
                  border: `1px solid ${summaryBorder}`,
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                }}>
                  <div>
                    <Text type="secondary" style={{ fontSize: isMobile ? 11 : 12, display: 'block' }}>
                      {t('recharge.payable_amount', '预计应付金额')}
                    </Text>
                    {!isMobile && <Text style={{ fontSize: 11, color: subColor }}>{t('recharge.realtime_billing', '实时按通道计费')}</Text>}
                  </div>
                  <div style={{ textAlign: 'right' }}>
                    <div style={{ fontSize: isMobile ? 20 : 24, fontWeight: 800, color: '#ef4444', lineHeight: 1 }}>
                      {needsConversion
                        ? `${paySymbol} ${payPreview.toFixed(payDecimals)}`
                        : `${currencySymbol} ${finalAmount.toFixed(2)}`}
                    </div>
                    {needsConversion && (
                      <Text style={{ fontSize: 11, color: subColor, display: 'block', marginTop: 4 }}>
                        {t('recharge.fx_pay_hint', {
                          defaultValue: '到账 {{credit}} · 1 {{from}} ≈ {{rate}} {{to}}',
                          credit: `${currencySymbol} ${finalAmount.toFixed(2)}`,
                          from: defaultCurrencyCode,
                          rate: Number((payRate as number).toPrecision(6)),
                          to: payCurrency,
                        })}
                      </Text>
                    )}
                  </div>
                </div>
          </Col>

          {/* 右侧 Column：选择支付方式 */}
          <Col xs={24} md={13} style={{
            padding: isMobile ? 0 : '0 0 0 12px',
            borderLeft: isMobile ? 'none' : `1px solid ${borderIdle}`,
            borderTop: isMobile ? `1px solid ${borderIdle}` : 'none',
            paddingTop: isMobile ? 12 : 0,
            display: 'flex',
            flexDirection: 'column',
            gap: isMobile ? 10 : 14,
          }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <Text strong style={{ fontSize: isMobile ? 13 : 14, color: labelColor }}>
                {t('recharge.payment_method', '选择支付方式')}
              </Text>
              <Text style={{ fontSize: 11, color: subColor }}>
                {t('recharge.selected', {
                  defaultValue: '已选 {{name}}',
                  name: paymentOptions.find(o => o.key === selectedChannel)?.name || t('recharge.payment_method', '支付方式'),
                })}
                {selectedChannel === 'allinpay' && paymentMethod === 'allinpay_wechat' ? ` · ${t('recharge.wechat_short', '微信')}` : ''}
                {selectedChannel === 'allinpay' && paymentMethod === 'allinpay_alipay' ? ` · ${t('recharge.alipay_short', '支付宝')}` : ''}
              </Text>
            </div>

            {/* 多支付方式网格列表（带自适应滚动容器） */}
            <div style={{
              maxHeight: isMobile ? undefined : 250,
              overflowY: isMobile ? 'visible' : 'auto',
              paddingRight: isMobile ? 0 : 4,
            }}>
              <Row gutter={[8, 8]}>
                {paymentOptions.map((opt) => {
                  const isSel = selectedChannel === opt.key;
                  return (
                    <Col span={12} key={opt.key}>
                      <div
                        onClick={() => applyChannelSelection(opt.channel)}
                        style={{
                          display: 'flex',
                          alignItems: 'center',
                          justifyContent: 'space-between',
                          padding: isMobile ? '8px 10px' : '10px 12px',
                          height: isMobile ? 46 : 52,
                          borderRadius: isMobile ? 10 : 12,
                          cursor: 'pointer',
                          border: `2px solid ${isSel ? opt.activeBorderColor : borderIdle}`,
                          background: isSel ? opt.activeBg : bgIdle,
                          transition: 'all 0.2s cubic-bezier(0.4, 0, 0.2, 1)',
                          position: 'relative',
                        }}
                      >
                        <div style={{ display: 'flex', alignItems: 'center', gap: 8, overflow: 'hidden' }}>
                          {opt.icon}
                          <div style={{ display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
                            <Text strong style={{
                              color: labelColor,
                              fontSize: isMobile ? 12 : 13,
                              lineHeight: 1.2,
                              whiteSpace: 'nowrap',
                              textOverflow: 'ellipsis',
                              overflow: 'hidden'
                            }}>
                              {opt.name}
                            </Text>
                            {opt.badge && (
                              <span style={{
                                fontSize: 10,
                                color: opt.badgeColor,
                                fontWeight: 600,
                                marginTop: 2,
                                lineHeight: 1,
                              }}>
                                {opt.badge}
                              </span>
                            )}
                          </div>
                        </div>

                        {/* 单选 Radio 选中指示器 */}
                        <div style={{
                          width: 18,
                          height: 18,
                          borderRadius: '50%',
                          border: `2px solid ${isSel ? opt.activeBorderColor : borderIdle}`,
                          display: 'flex',
                          alignItems: 'center',
                          justifyContent: 'center',
                          background: isSel ? opt.activeBorderColor : 'transparent',
                          transition: 'all 0.2s ease',
                          flexShrink: 0,
                          marginLeft: 4,
                        }}>
                          {isSel && (
                            <div style={{ width: 6, height: 6, borderRadius: '50%', background: '#ffffff' }} />
                          )}
                        </div>
                      </div>
                    </Col>
                  );
                })}
              </Row>
            </div>

            {showAllinpaySubPicker && (
              <div style={{
                padding: 12,
                borderRadius: 12,
                border: `1px solid ${borderIdle}`,
                background: bgIdle,
              }}>
                <Text style={{ fontSize: 12, color: subColor, display: 'block', marginBottom: 8 }}>
                  {t('recharge.allinpay_pick', '请选择通联支付方式')}
                </Text>
                <Row gutter={[10, 10]}>
                  {allinpayMethods.map((method) => {
                    const isSel = paymentMethod === method;
                    const isWechat = method === 'allinpay_wechat';
                    const accent = isWechat ? '#07c160' : '#1677ff';
                    return (
                      <Col span={12} key={method}>
                        <div
                          onClick={() => setPaymentMethod(method)}
                          style={{
                            display: 'flex',
                            alignItems: 'center',
                            gap: 8,
                            padding: '10px 12px',
                            borderRadius: 10,
                            cursor: 'pointer',
                            border: `2px solid ${isSel ? accent : borderIdle}`,
                            background: isSel ? (isLight ? `${accent}0f` : `${accent}26`) : 'transparent',
                          }}
                        >
                          {isWechat
                            ? <WechatOutlined style={{ fontSize: 20, color: accent }} />
                            : <AlipayCircleOutlined style={{ fontSize: 20, color: accent }} />}
                          <Text strong style={{ color: labelColor, fontSize: 13 }}>
                            {isWechat ? t('recharge.wechat_pay', '微信支付') : t('recharge.alipay', '支付宝')}
                          </Text>
                        </div>
                      </Col>
                    );
                  })}
                </Row>
              </div>
            )}

            {/* 错误提示 */}
            {errorMessage && (
              <div style={{
                padding: '8px 12px',
                background: 'rgba(239, 68, 68, 0.08)',
                border: '1px solid rgba(239, 68, 68, 0.2)',
                borderRadius: 10,
                textAlign: 'center',
              }}>
                <Text type="danger" style={{ fontSize: 12, fontWeight: 500 }}>⚠️ {errorMessage}</Text>
              </div>
            )}

            {/* 支付触发主按钮 */}
            <Button
              type="primary"
              block
              size="large"
              loading={loading}
              onClick={handleCreateOrder}
              disabled={finalAmount < 0.01}
              className="recharge-pay-btn"
              style={{
                marginTop: isMobile ? 4 : 'auto',
                borderRadius: 12,
                height: isMobile ? 44 : 48,
                fontSize: isMobile ? 14 : 15,
                fontWeight: 600,
                background: getPayButtonBackground(paymentMethod),
                color: '#ffffff',
                border: 'none',
                boxShadow: getPayButtonShadow(paymentMethod),
                transition: 'all 0.25s ease',
              }}
            >
              {(() => {
                const btnFg = { color: '#ffffff' };
                if (paymentMethod === 'hyperbc') {
                  return <Space style={btnFg}><span style={btnFg}>₿</span>{t('recharge.go_hyperbc', '去 HyperBC 支付')}</Space>;
                }
                if (paymentMethod === 'stripe') {
                  return <Space style={btnFg}><CreditCardOutlined style={btnFg} />{t('recharge.go_stripe', '去 Stripe 支付')}</Space>;
                }
                if (paymentMethod === 'alipay' || paymentMethod === 'allinpay_alipay') {
                  return (
                    <Space style={btnFg}>
                      <AlipayCircleOutlined style={btnFg} />
                      {paymentMethod === 'alipay'
                        ? t('recharge.go_alipay', '去支付宝支付')
                        : t('recharge.gen_alipay_qr', '生成支付宝支付码')}
                    </Space>
                  );
                }
                if (paymentMethod === 'wechat' || paymentMethod === 'allinpay_wechat') {
                  return <Space style={btnFg}><WechatOutlined style={btnFg} />{t('recharge.gen_wechat_qr', '生成微信支付码')}</Space>;
                }
                return <Space style={btnFg}><WalletOutlined style={btnFg} />{t('recharge.go_pay', '去支付')}</Space>;
              })()}
            </Button>

            {/* 安全验证 Footer */}
            <div style={{ textAlign: 'center', display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 6 }}>
              <SafetyCertificateOutlined style={{ fontSize: 12, color: '#10b981' }} />
              <Text type="secondary" style={{ fontSize: 11 }}>
                {t('recharge.trust_badge', '资金安全保障 · 充值后即时到账 · 正规支付渠道')}
              </Text>
            </div>
          </Col>
        </Row>
        </>
      )}
    </Modal>
  );
};

export default RechargeModal;
