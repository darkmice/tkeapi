/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useState, useEffect, useRef } from 'react';
import { useNavigate, Link, useSearchParams } from 'react-router-dom';
import { message } from 'antd';
import { 
  User, 
  Mail, 
  Phone, 
  Lock, 
  Eye, 
  EyeOff, 
  Loader2, 
  ShieldAlert,
  KeyRound
} from 'lucide-react';
import request from '../../utils/request';
import { apiErrMsg } from '../../utils/apiErr';
import { resolveInviteParams, trackMarketingLinkClick } from '../../utils/inviteTracking';
import { browserTimezone } from '../../utils/timedisplay';
import { useTranslation } from 'react-i18next';
import useAuthStore from '../../store/auth';
import useSettingsStore from '../../store/settings';
import AuthLayout from '../../layouts/AuthLayout';
import SignedInEntryGate from './SignedInEntryGate';
import type { AuthMethodOption } from '../../layouts/AuthLayout';
import { WechatOutlined } from '@ant-design/icons';
import WechatQR from '../../components/WechatQR';
import GoogleIcon from '../../components/GoogleIcon';
import { stripAuthParamsFromUrl } from '../../utils/safeRedirect';

/** 发送成功后的倒计时（秒） */
const CODE_COOLDOWN_SUCCESS = 60;
/** 发送失败后的短冷却，防止连点刷错误弹窗（秒） */
const CODE_COOLDOWN_ERROR = 3;

const Register: React.FC = () => {
  const { t, i18n } = useTranslation();
  const [loading, setLoading] = useState(false);
  const [countdown, setCountdown] = useState(0);
  const [sendingCode, setSendingCode] = useState(false);
  const sendingCodeRef = useRef(false);
  const cooldownUntilRef = useRef(0);
  const [activeTab, setActiveTab] = useState('');
  
  // 密码显示状态
  const [showPassword, setShowPassword] = useState(false);
  const [showConfirmPassword, setShowConfirmPassword] = useState(false);

  // 表单状态值
  const [usernameVal, setUsernameVal] = useState('');
  const [passwordVal, setPasswordVal] = useState('');
  const [confirmPasswordVal, setConfirmPasswordVal] = useState('');
  const [emailVal, setEmailVal] = useState('');
  const [mobileVal, setMobileVal] = useState('');
  const [codeVal, setCodeVal] = useState('');

  // 校验错误状态
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [formError, setFormError] = useState('');
  const [shakeKey, setShakeKey] = useState(0);
  const [contactConflict, setContactConflict] = useState<{
    code: 'CONTACT_IN_USE' | 'CONTACT_AT_LIMIT';
    boundCount?: number;
    limit?: number;
  } | null>(null);
  const [agreed, setAgreed] = useState(false);
  const [wechatState, setWechatState] = useState('');
  const [wechatTicket, setWechatTicket] = useState('');

  const navigate = useNavigate();
  const { setToken, setUser } = useAuthStore();
  const { settings, fetchSettings } = useSettingsStore();
  const [searchParams] = useSearchParams();

  // URL 优先，否则读 3 天内未清缓存的邀请参数（localStorage + cookie）
  const { aff, team } = resolveInviteParams(searchParams.toString() ? `?${searchParams.toString()}` : '');

  useEffect(() => { 
    void fetchSettings(true); 
  }, []);

  // 统计邀请链接点击：仅当 URL 带 aff（真实点开邀请链接）；主题页回退 from=promo 不计邀请
  useEffect(() => {
    const urlAff = searchParams.get('aff')?.trim() || '';
    if (!urlAff) return;
    if (searchParams.get('from') === 'promo') return;
    const urlTeam = searchParams.get('team')?.trim() || '';
    if (urlTeam) {
      void trackMarketingLinkClick({ linkType: 'team_invite', aff: urlAff, linkKey: urlTeam });
    } else {
      void trackMarketingLinkClick({ linkType: 'invite', aff: urlAff });
    }
  }, [searchParams]);

  const getAgreementUrl = (type: 'tos' | 'privacy') => {
    const agreement = settings?.agreement;
    if (!agreement) return '#';
    const isZh = i18n.language.startsWith('zh');
    if (type === 'tos') {
      const mode = isZh ? agreement.tos_mode : agreement.tos_mode_en;
      if (mode === 'link') {
        const rawLink = isZh ? agreement.tos_link : (agreement.tos_link_en || agreement.tos_link);
        const link = String(rawLink || '').trim();
        return link ? (/^https?:\/\//i.test(link) || link.startsWith('/') ? link : `https://${link}`) : '#';
      }
      return isZh ? '/legal/terms' : '/legal/terms?lang=en';
    } else {
      const mode = isZh ? agreement.privacy_mode : agreement.privacy_mode_en;
      if (mode === 'link') {
        const rawLink = isZh ? agreement.privacy_link : (agreement.privacy_link_en || agreement.privacy_link);
        const link = String(rawLink || '').trim();
        return link ? (/^https?:\/\//i.test(link) || link.startsWith('/') ? link : `https://${link}`) : '#';
      }
      return isZh ? '/legal/privacy' : '/legal/privacy?lang=en';
    }
  };

  useEffect(() => {
    if (countdown > 0) { 
      const timer = setTimeout(() => setCountdown(c => c - 1), 1000); 
      return () => clearTimeout(timer); 
    }
  }, [countdown]);

  const startCooldown = (seconds: number) => {
    cooldownUntilRef.current = Date.now() + seconds * 1000;
    setCountdown(seconds);
  };

  const isBlocked = () =>
    sendingCodeRef.current || Date.now() < cooldownUntilRef.current || countdown > 0;

  // 已登录用户点击团队邀请链接时自动加入团队
  const { token } = useAuthStore();
  useEffect(() => {
    if (token && team) {
      (async () => {
        try {
          const res = await (request.post('/team-marketing/join', { invite_code: team }) as any);
          if (res.status === 'joined') {
            message.success('成功加入团队！');
          } else if (res.status === 'already_member') {
            message.info('您已是该团队成员');
          }
          navigate('/dashboard');
        } catch (e: any) {
          message.error(e?.response?.data?.error?.message || '加入团队失败');
          navigate('/dashboard');
        }
      })();
    }
  }, [token, team]);

  const reg = settings?.registration;
  const siteLogo = settings?.site?.logo;

  const regTabs: { key: string; label: string; icon: React.ReactNode }[] = [];
  if (reg?.enable_username_registration) {
    regTabs.push({ key: 'username', label: t('auth.username_reg'), icon: <User className="w-3.5 h-3.5" /> });
  }
  if (reg?.enable_mobile_registration) {
    regTabs.push({ key: 'mobile', label: t('auth.mobile_reg'), icon: <Phone className="w-3.5 h-3.5" /> });
  }
  if (reg?.enable_email_registration) {
    regTabs.push({ key: 'email', label: t('auth.email_reg'), icon: <Mail className="w-3.5 h-3.5" /> });
  }

  // 默认 Tab 设置
  useEffect(() => {
    if (activeTab === 'wechat' && settings?.login?.enable_wechat_login) return;
    if (regTabs.length > 0 && (!activeTab || !regTabs.some(tab => tab.key === activeTab))) {
      setActiveTab(regTabs[0].key);
    } else if (regTabs.length === 0 && settings?.login?.enable_wechat_login) {
      setActiveTab('wechat');
    }
  }, [reg, settings?.login?.enable_wechat_login]);

  const noRegistration = regTabs.length === 0 && !settings?.login?.enable_wechat_login;

  // 发送邮箱验证码
  const sendEmailCode = async () => {
    if (isBlocked()) return;
    if (!emailVal.trim()) {
      setErrors(prev => ({ ...prev, email: t('auth.email_required') }));
      setShakeKey(k => k + 1);
      return;
    }
    const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
    if (!emailRegex.test(emailVal.trim())) {
      setErrors(prev => ({ ...prev, email: t('auth.email_invalid') }));
      setShakeKey(k => k + 1);
      return;
    }

    sendingCodeRef.current = true;
    setSendingCode(true);
    setFormError('');
    try {
      await request.post('/auth/send-code', { email: emailVal.trim(), purpose: 'register' }, { skipErrorHandler: true } as any);
      message.success(t('auth.code_sent'));
      startCooldown(CODE_COOLDOWN_SUCCESS);
    } catch (e: any) {
      setFormError(apiErrMsg(e, t('common.error')));
      setShakeKey(k => k + 1);
      startCooldown(CODE_COOLDOWN_ERROR);
    } finally {
      sendingCodeRef.current = false;
      setSendingCode(false);
    }
  };

  // 发送手机验证码
  const sendSmsCode = async () => {
    if (isBlocked()) return;
    if (!mobileVal.trim()) {
      setErrors(prev => ({ ...prev, mobile: t('auth.mobile_required') }));
      setShakeKey(k => k + 1);
      return;
    }

    sendingCodeRef.current = true;
    setSendingCode(true);
    setFormError('');
    try {
      await request.post('/auth/send-sms-code', { mobile: mobileVal.trim(), purpose: 'register' }, { skipErrorHandler: true } as any);
      message.success(t('auth.sms_code_sent'));
      startCooldown(CODE_COOLDOWN_SUCCESS);
    } catch (e: any) {
      setFormError(apiErrMsg(e, t('common.error')));
      setShakeKey(k => k + 1);
      startCooldown(CODE_COOLDOWN_ERROR);
    } finally {
      sendingCodeRef.current = false;
      setSendingCode(false);
    }
  };

  const handleTabChange = (key: string) => {
    setActiveTab(key);
    setErrors({});
    setFormError('');
    setContactConflict(null);
    setWechatTicket('');
    setShowPassword(false);
    setShowConfirmPassword(false);
    setAgreed(false);
  };

  const toFormError = (error: unknown) => {
    const msg = apiErrMsg(error, t('common.error'));
    if (msg === 'User already exists' || msg === '请更换用户名尝试') {
      return t('auth.user_already_exists');
    }
    return msg;
  };

  const goLoginWithCurrentContact = () => {
    if (activeTab === 'wechat' && wechatTicket) {
      navigate(`/login?code=${encodeURIComponent(wechatTicket)}&type=wechat`);
      return;
    }
    const params = new URLSearchParams();
    if (activeTab === 'email' && emailVal.trim()) {
      params.set('type', 'email');
      params.set('identifier', emailVal.trim());
    } else if (activeTab === 'mobile' && mobileVal.trim()) {
      params.set('type', 'mobile');
      params.set('identifier', mobileVal.trim());
    }
    const qs = params.toString();
    navigate(qs ? `/login?${qs}` : '/login');
  };

  const submitRegister = async (confirmNewAccount = false) => {
    let res: any;
    if (activeTab === 'wechat' && wechatTicket) {
      res = await request.post('/auth/exchange-login-code', {
        code: wechatTicket,
        intent: 'register',
        confirm_new_account: confirmNewAccount,
      }, { skipErrorHandler: true } as any);
      setWechatTicket('');
      setToken(res.token);
      setUser(res.user);
      message.success(t('auth.register_success'));
      navigate('/dashboard');
      return;
    }
    const commonPayload = {
      password: passwordVal,
      timezone: browserTimezone(),
      ...(aff ? { aff } : {}),
      ...(team ? { team } : {}),
      ...(confirmNewAccount ? { confirm_new_account: true } : {}),
    };
    
    if (activeTab === 'username') {
      res = await request.post('/auth/register', { username: usernameVal.trim(), ...commonPayload }, { skipErrorHandler: true } as any);
    } else if (activeTab === 'email') {
      res = await request.post('/auth/register-email', { email: emailVal.trim(), code: codeVal.trim(), ...commonPayload }, { skipErrorHandler: true } as any);
    } else if (activeTab === 'mobile') {
      res = await request.post('/auth/register-mobile', { mobile: mobileVal.trim(), code: codeVal.trim(), ...commonPayload }, { skipErrorHandler: true } as any);
    }

    setToken(res.token); 
    setUser(res.user);
    message.success(t('auth.register_success')); 
    navigate('/dashboard');
  };

  const handleContactConflict = (err: any) => {
    const code = err?.error_code;
    if (code === 'CONTACT_IN_USE' || code === 'CONTACT_AT_LIMIT') {
      setContactConflict({
        code,
        boundCount: err?.bound_count,
        limit: err?.limit,
      });
      setFormError('');
      return true;
    }
    return false;
  };

  const continueNewAccount = async () => {
    if (loading) return;
    setLoading(true);
    setFormError('');
    try {
      await submitRegister(true);
    } catch (e: any) {
      const err = e?.response?.data?.error;
      if (!handleContactConflict(err)) {
        setFormError(toFormError(e));
        setShakeKey(k => k + 1);
      }
    } finally {
      setTimeout(() => setLoading(false), 800);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (loading) return;

    // 前端表单校验
    const tempErrors: Record<string, string> = {};
    if (activeTab === 'username') {
      if (!usernameVal.trim()) tempErrors.username = t('auth.username_required');
      else if (usernameVal.trim().length > 48) tempErrors.username = t('auth.username_max_length');
      if (!passwordVal) tempErrors.password = t('auth.password_required');
      if (!confirmPasswordVal) tempErrors.confirm_password = t('auth.confirm_password_required');
      else if (passwordVal !== confirmPasswordVal) tempErrors.confirm_password = t('auth.passwords_not_match');
    } else if (activeTab === 'email') {
      if (!emailVal.trim()) tempErrors.email = t('auth.email_required');
      else if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(emailVal.trim())) tempErrors.email = t('auth.email_invalid');
      if (!codeVal.trim()) tempErrors.code = t('auth.code_required');
      if (!passwordVal) tempErrors.password = t('auth.password_required');
      if (!confirmPasswordVal) tempErrors.confirm_password = t('auth.confirm_password_required');
      else if (passwordVal !== confirmPasswordVal) tempErrors.confirm_password = t('auth.passwords_not_match');
    } else if (activeTab === 'mobile') {
      if (!mobileVal.trim()) tempErrors.mobile = t('auth.mobile_required');
      if (!codeVal.trim()) tempErrors.code = t('auth.code_required');
      if (!passwordVal) tempErrors.password = t('auth.password_required');
      if (!confirmPasswordVal) tempErrors.confirm_password = t('auth.confirm_password_required');
      else if (passwordVal !== confirmPasswordVal) tempErrors.confirm_password = t('auth.passwords_not_match');
    }

    const needTos = settings?.agreement?.tos_enabled === true;
    const needPrivacy = settings?.agreement?.privacy_enabled === true;
    if ((needTos || needPrivacy) && !agreed) {
      tempErrors.agree = i18n.language.startsWith('zh') ? '您必须阅读并同意协议才能继续' : 'You must agree to the agreement to continue';
    }

    if (Object.keys(tempErrors).length > 0) {
      setErrors(tempErrors);
      setFormError('');
      setShakeKey(k => k + 1); // 触发抖动
      return;
    }
    setErrors({});
    setFormError('');
    setLoading(true);

    try {
      await submitRegister(false);
    } catch (e: any) {
      const err = e?.response?.data?.error;
      if (!handleContactConflict(err)) {
        setFormError(toFormError(e));
        setShakeKey(k => k + 1);
      }
    } finally { 
      setTimeout(() => setLoading(false), 800); 
    }
  };

  const showWechat = settings?.login?.enable_wechat_login;
  const showGoogle = settings?.login?.enable_google_login;

  const wechatAppId = settings?.wechat_oauth_app_id || '';
  const wechatBaseOrigin = settings?.wechat_oauth_callback_domain || window.location.origin;
  const wechatRedirectUri = `${wechatBaseOrigin}/api/v1/auth/oauth/wechat/callback`;

  useEffect(() => {
    const ticket = searchParams.get('wechat_ticket')?.trim() || '';
    if (!ticket) return;
    stripAuthParamsFromUrl();
    setWechatTicket(ticket);
    setActiveTab('wechat');
    setLoading(true);
    request
      .post('/auth/exchange-login-code', {
        code: ticket,
        intent: 'register',
        confirm_new_account: false,
      }, { skipErrorHandler: true } as any)
      .then((res: any) => {
        if (res?.token) {
          setWechatTicket('');
          setToken(res.token);
          setUser(res.user);
          message.success(t('auth.register_success'));
          navigate('/dashboard');
        }
      })
      .catch((e: any) => {
        const err = e?.response?.data?.error;
        if (!handleContactConflict(err)) {
          setFormError(toFormError(e));
          setShakeKey(k => k + 1);
        }
      })
      .finally(() => {
        setLoading(false);
      });
  }, [searchParams]);

  useEffect(() => {
    if (!showWechat) return;
    let cancelled = false;
    const params: Record<string, string> = { 
      provider: 'wechat',
      origin: window.location.origin,
      intent: 'register',
    };
    if (aff) params.aff = aff;
    if (team) params.team = team;
    const tz = browserTimezone();
    if (tz) params.timezone = tz;
    request
      .get('/auth/oauth/state', { params, skipErrorHandler: true } as any)
      .then((res: any) => {
        if (!cancelled && res?.state) setWechatState(res.state);
      })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [showWechat, aff, team, settings?.wechat_oauth_callback_domain]);

  const handleGoogleLogin = () => {
    // 发起时带上 aff/team，后端写入 OAuth state；cookie 作兜底
    const params = new URLSearchParams();
    if (aff) params.set('aff', aff);
    if (team) params.set('team', team);
    const tz = browserTimezone();
    if (tz) params.set('timezone', tz);
    const qs = params.toString();
    window.location.href = `/api/v1/auth/oauth/google${qs ? `?${qs}` : ''}`;
  };

  const layoutMethods: AuthMethodOption[] = [];
  if (showWechat) {
    layoutMethods.push({ 
      key: 'wechat', 
      label: '微信扫码注册', 
      icon: <WechatOutlined style={{ fontSize: '18px', color: '#07C160' }} /> 
    });
  }
  if (showGoogle) {
    layoutMethods.push({ 
      key: 'google', 
      label: 'Google 注册', 
      icon: <GoogleIcon className="w-4 h-4" />,
      onClick: handleGoogleLogin
    });
  }
  // 将所有的表单通道以图标形式追加入 layoutMethods
  regTabs.forEach(tab => {
    const isActive = tab.key === activeTab;
    layoutMethods.push({
      key: tab.key,
      label: tab.label,
      icon: React.cloneElement(tab.icon as React.ReactElement<any>, { 
        className: `w-4.5 h-4.5 transition-colors ${
          isActive 
            ? 'text-zinc-900 dark:text-zinc-50 font-semibold' 
            : 'text-zinc-500 hover:text-zinc-900 dark:text-zinc-400 dark:hover:text-zinc-100'
        }` 
      }),
      onClick: () => handleTabChange(tab.key)
    });
  });

  const bottomLinks = (
    <div className="flex items-center gap-1.5 text-xs text-zinc-500 dark:text-zinc-400">
      <span>{t('auth.have_account')}</span>
      <Link to="/login" className="!text-zinc-900 dark:!text-zinc-100 hover:underline font-medium transition-colors">
        {t('auth.login_link')}
      </Link>
    </div>
  );

  const baseTitle = settings?.site?.login_title || settings?.site?.name || 'Tkeapi';
  const registerTitle = `${baseTitle} ${i18n.language.startsWith('zh') ? '注册' : 'Register'}`;
  const registerTitleUrl = settings?.site?.login_title_url || '';
  const registerSubtitle = settings?.site?.login_subtitle || 'Next-gen LLM API Gateway';

  return (
    <SignedInEntryGate enabled={!team} hold={Boolean(token && team)}>
    <AuthLayout
      title={registerTitle}
      subtitle={registerSubtitle}
      titleHref={registerTitleUrl}
      logo={siteLogo}
      loading={!settings}
      methodsLabel={t('auth.register_method')}
      methods={layoutMethods.length > 0 ? layoutMethods : undefined}
      activeMethod={activeTab}
      onMethodChange={handleTabChange}
      bottomLinks={bottomLinks}
    >
      {contactConflict ? (
        <div className="space-y-4 mt-2">
          <div className="w-full rounded-md bg-amber-500/10 border border-amber-500/20 dark:bg-amber-500/15 dark:border-amber-500/30 px-3.5 py-2.5 text-xs font-medium text-amber-800 dark:text-amber-300 leading-relaxed text-center shadow-xs">
            {contactConflict.code === 'CONTACT_AT_LIMIT'
              ? t('auth.contact_at_limit_body', { count: contactConflict.boundCount, limit: contactConflict.limit })
              : activeTab === 'wechat'
                ? t('auth.contact_in_use_wechat')
                : activeTab === 'mobile'
                  ? t('auth.contact_in_use_mobile')
                  : t('auth.contact_in_use_email')}
          </div>
          {formError && (
            <p key={`form-${shakeKey}`} className="text-[12px] font-medium text-destructive text-center animate-shake">{formError}</p>
          )}
          {contactConflict.code === 'CONTACT_IN_USE' ? (
            <>
              <button
                type="button"
                disabled={loading}
                onClick={continueNewAccount}
                className="inline-flex items-center justify-center rounded-md text-sm font-semibold transition-all duration-200 focus-visible:outline-none focus-visible:ring-1 disabled:pointer-events-none disabled:opacity-50 bg-zinc-900 text-zinc-50 hover:bg-zinc-900/90 dark:bg-zinc-50 dark:text-zinc-900 dark:hover:bg-zinc-50/90 shadow-sm h-9 px-4 py-2 w-full cursor-pointer"
              >
                {loading && (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                )}
                {t('auth.continue_register')}
              </button>
              <button
                type="button"
                disabled={loading}
                onClick={goLoginWithCurrentContact}
                className="inline-flex items-center justify-center rounded-md text-sm font-semibold transition-all duration-200 border border-zinc-200 dark:border-zinc-800 text-zinc-900 dark:text-zinc-100 hover:bg-zinc-100 dark:hover:bg-zinc-900 h-9 px-4 py-2 w-full cursor-pointer disabled:opacity-50"
              >
                {t('auth.go_login')}
              </button>
            </>
          ) : (
            <>
              <button
                type="button"
                disabled={loading}
                onClick={goLoginWithCurrentContact}
                className="inline-flex items-center justify-center rounded-md text-sm font-semibold transition-all duration-200 focus-visible:outline-none focus-visible:ring-1 disabled:pointer-events-none disabled:opacity-50 bg-zinc-900 text-zinc-50 hover:bg-zinc-900/90 dark:bg-zinc-50 dark:text-zinc-900 dark:hover:bg-zinc-50/90 shadow-sm h-9 px-4 py-2 w-full cursor-pointer"
              >
                {t('auth.go_login')}
              </button>
              <button
                type="button"
                disabled={loading}
                onClick={() => {
                  setContactConflict(null);
                  setWechatTicket('');
                  setFormError('');
                }}
                className="text-xs text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200 w-full text-center transition-colors cursor-pointer disabled:opacity-50"
              >
                {t('common.back')}
              </button>
            </>
          )}
        </div>
      ) : activeTab === 'wechat' ? (
        <div className="flex flex-col items-center justify-center p-2 bg-background border border-zinc-200 dark:border-zinc-800 rounded-lg shadow-xs mt-2">
          {wechatState ? (
            <WechatQR appId={wechatAppId} redirectUri={wechatRedirectUri} state={wechatState} />
          ) : (
            <div className="py-10 text-zinc-500 text-sm">加载中...</div>
          )}
          {regTabs.length > 0 && (
            <button 
              type="button" 
              onClick={() => handleTabChange(regTabs[0]?.key || 'username')}
              className="text-xs text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200 mt-2 transition-colors cursor-pointer"
            >
              {i18n.language.startsWith('zh') ? '返回注册方式' : 'Back to registration'}
            </button>
          )}
        </div>
      ) : noRegistration ? (
        <div className="flex flex-col items-center justify-center py-6 text-center space-y-2 text-zinc-500">
          <ShieldAlert className="w-10 h-10 text-zinc-400 dark:text-zinc-600" />
          <p className="text-sm font-medium">{t('auth.registration_disabled')}</p>
        </div>
      ) : (
        <form onSubmit={handleSubmit} className="space-y-4 mt-2">
          {regTabs.length > 1 && (
            <div
              className={`grid w-full gap-1 p-1 bg-zinc-100 dark:bg-zinc-900 border border-zinc-200/50 dark:border-zinc-800/40 rounded-lg text-xs font-medium text-zinc-500 select-none ${
                regTabs.length === 3 ? 'grid-cols-3' : 'grid-cols-2'
              }`}
            >
              {regTabs.map(tab => {
                const isSelected = activeTab === tab.key;
                return (
                  <button
                    key={tab.key}
                    type="button"
                    onClick={() => handleTabChange(tab.key)}
                    className={`flex items-center justify-center gap-1.5 py-1.5 px-2 rounded-md transition-all duration-200 cursor-pointer
                      ${isSelected
                        ? 'bg-background text-zinc-900 dark:text-zinc-100 shadow-xs border border-zinc-200/50 dark:border-zinc-800/60 font-semibold'
                        : 'hover:text-zinc-800 dark:hover:text-zinc-200'
                      }`}
                  >
                    {tab.icon}
                    <span>{tab.label}</span>
                  </button>
                );
              })}
            </div>
          )}

          {/* 1. 用户名注册输入字段 */}
          {activeTab === 'username' && (
            <div className="space-y-1.5 text-left">
              <label className="text-[13px] font-medium text-zinc-900 dark:text-zinc-100">
                {t('login.username')}
              </label>
              <input
                type="text"
                placeholder={t('auth.username_placeholder')}
                maxLength={48}
                value={usernameVal}
                onChange={(e) => {
                  setUsernameVal(e.target.value);
                  if (errors.username) setErrors(prev => ({ ...prev, username: '' }));
                }}
                className={`flex h-9 w-full rounded-md border bg-transparent px-3 py-1 text-sm shadow-xs transition-all duration-200 placeholder:text-zinc-400/70 dark:placeholder:text-zinc-600/70 focus-visible:outline-none focus-visible:ring-1 disabled:cursor-not-allowed disabled:opacity-50
                  ${errors.username ? 'border-destructive focus-visible:ring-destructive focus-visible:border-destructive' : 'border-zinc-200 dark:border-zinc-800 focus-visible:border-zinc-950 dark:focus-visible:border-zinc-300 focus-visible:ring-zinc-950/10 dark:focus-visible:ring-zinc-300/10'}`}
              />
              {errors.username && (
                <p key={`reg-u-${shakeKey}`} className="text-[11px] font-medium text-destructive animate-shake">{errors.username}</p>
              )}
            </div>
          )}

          {/* 2. 邮箱注册输入字段 */}
          {activeTab === 'email' && (
            <>
              <div className="space-y-1.5 text-left">
                <label className="text-[13px] font-medium text-zinc-900 dark:text-zinc-100">
                  {t('users.email')}
                </label>
                <input
                  type="email"
                  placeholder={t('auth.email_placeholder')}
                  value={emailVal}
                  onChange={(e) => {
                    setEmailVal(e.target.value);
                    if (errors.email) setErrors(prev => ({ ...prev, email: '' }));
                  }}
                  className={`flex h-9 w-full rounded-md border bg-transparent px-3 py-1 text-sm shadow-xs transition-all duration-200 placeholder:text-zinc-400/70 dark:placeholder:text-zinc-600/70 focus-visible:outline-none focus-visible:ring-1 disabled:cursor-not-allowed disabled:opacity-50
                    ${errors.email ? 'border-destructive focus-visible:ring-destructive focus-visible:border-destructive' : 'border-zinc-200 dark:border-zinc-800 focus-visible:border-zinc-950 dark:focus-visible:border-zinc-300 focus-visible:ring-zinc-950/10 dark:focus-visible:ring-zinc-300/10'}`}
                />
                {errors.email && (
                  <p key={`reg-e-${shakeKey}`} className="text-[11px] font-medium text-destructive animate-shake">{errors.email}</p>
                )}
              </div>

              <div className="space-y-1.5 text-left">
                <label className="text-[13px] font-medium text-zinc-900 dark:text-zinc-100">
                  {t('auth.code_placeholder')}
                </label>
                <div className="relative">
                  <input
                    type="text"
                    placeholder={t('auth.code_placeholder')}
                    value={codeVal}
                    onChange={(e) => {
                      setCodeVal(e.target.value);
                      if (errors.code) setErrors(prev => ({ ...prev, code: '' }));
                    }}
                    className={`flex h-9 w-full rounded-md border bg-transparent pl-3 pr-24 py-1 text-sm shadow-xs transition-all duration-200 placeholder:text-zinc-400/70 dark:placeholder:text-zinc-600/70 focus-visible:outline-none focus-visible:ring-1 disabled:cursor-not-allowed disabled:opacity-50
                      ${errors.code ? 'border-destructive focus-visible:ring-destructive focus-visible:border-destructive' : 'border-zinc-200 dark:border-zinc-800 focus-visible:border-zinc-950 dark:focus-visible:border-zinc-300 focus-visible:ring-zinc-950/10 dark:focus-visible:ring-zinc-300/10'}`}
                  />
                  <button
                    type="button"
                    disabled={countdown > 0 || sendingCode || !emailVal.trim()}
                    onClick={sendEmailCode}
                    className={`absolute right-1.5 top-1 h-7 px-2.5 rounded text-[11px] font-semibold border transition-all duration-200 flex items-center justify-center disabled:cursor-not-allowed disabled:opacity-50 disabled:pointer-events-none
                      ${emailVal.trim() && countdown === 0 && !sendingCode
                        ? 'bg-zinc-100 dark:bg-zinc-900 border-zinc-200 dark:border-zinc-800 text-zinc-900 dark:text-zinc-50 hover:bg-zinc-200 dark:hover:bg-zinc-800 cursor-pointer' 
                        : 'bg-transparent border-transparent text-zinc-400 dark:text-zinc-600'
                      }`}
                  >
                    {sendingCode ? (
                      <Loader2 className="w-3.5 h-3.5 animate-spin" />
                    ) : countdown > 0 ? (
                      `${countdown}s`
                    ) : (
                      t('auth.send_code')
                    )}
                  </button>
                </div>
                {errors.code && (
                  <p key={`reg-ec-${shakeKey}`} className="text-[11px] font-medium text-destructive animate-shake">{errors.code}</p>
                )}
              </div>
            </>
          )}

          {/* 3. 手机号注册输入字段 */}
          {activeTab === 'mobile' && (
            <>
              <div className="space-y-1.5 text-left">
                <label className="text-[13px] font-medium text-zinc-900 dark:text-zinc-100">
                  {t('auth.mobile_reg').replace('注册', '')}
                </label>
                <input
                  type="tel"
                  placeholder={t('auth.mobile_placeholder')}
                  value={mobileVal}
                  onChange={(e) => {
                    setMobileVal(e.target.value);
                    if (errors.mobile) setErrors(prev => ({ ...prev, mobile: '' }));
                  }}
                  className={`flex h-9 w-full rounded-md border bg-transparent px-3 py-1 text-sm shadow-xs transition-all duration-200 placeholder:text-zinc-400/70 dark:placeholder:text-zinc-600/70 focus-visible:outline-none focus-visible:ring-1 disabled:cursor-not-allowed disabled:opacity-50
                    ${errors.mobile ? 'border-destructive focus-visible:ring-destructive focus-visible:border-destructive' : 'border-zinc-200 dark:border-zinc-800 focus-visible:border-zinc-950 dark:focus-visible:border-zinc-300 focus-visible:ring-zinc-950/10 dark:focus-visible:ring-zinc-300/10'}`}
                />
                {errors.mobile && (
                  <p key={`reg-m-${shakeKey}`} className="text-[11px] font-medium text-destructive animate-shake">{errors.mobile}</p>
                )}
              </div>

              <div className="space-y-1.5 text-left">
                <label className="text-[13px] font-medium text-zinc-900 dark:text-zinc-100">
                  {t('auth.code_placeholder')}
                </label>
                <div className="relative">
                  <input
                    type="text"
                    placeholder={t('auth.code_placeholder')}
                    value={codeVal}
                    onChange={(e) => {
                      setCodeVal(e.target.value);
                      if (errors.code) setErrors(prev => ({ ...prev, code: '' }));
                    }}
                    className={`flex h-9 w-full rounded-md border bg-transparent pl-3 pr-24 py-1 text-sm shadow-xs transition-all duration-200 placeholder:text-zinc-400/70 dark:placeholder:text-zinc-600/70 focus-visible:outline-none focus-visible:ring-1 disabled:cursor-not-allowed disabled:opacity-50
                      ${errors.code ? 'border-destructive focus-visible:ring-destructive focus-visible:border-destructive' : 'border-zinc-200 dark:border-zinc-800 focus-visible:border-zinc-950 dark:focus-visible:border-zinc-300 focus-visible:ring-zinc-950/10 dark:focus-visible:ring-zinc-300/10'}`}
                  />
                  <button
                    type="button"
                    disabled={countdown > 0 || sendingCode || !mobileVal.trim()}
                    onClick={sendSmsCode}
                    className={`absolute right-1.5 top-1 h-7 px-2.5 rounded text-[11px] font-semibold border transition-all duration-200 flex items-center justify-center disabled:cursor-not-allowed disabled:opacity-50 disabled:pointer-events-none
                      ${mobileVal.trim() && countdown === 0 && !sendingCode
                        ? 'bg-zinc-100 dark:bg-zinc-900 border-zinc-200 dark:border-zinc-800 text-zinc-900 dark:text-zinc-50 hover:bg-zinc-200 dark:hover:bg-zinc-800 cursor-pointer' 
                        : 'bg-transparent border-transparent text-zinc-400 dark:text-zinc-600'
                      }`}
                  >
                    {sendingCode ? (
                      <Loader2 className="w-3.5 h-3.5 animate-spin" />
                    ) : countdown > 0 ? (
                      `${countdown}s`
                    ) : (
                      t('auth.send_code')
                    )}
                  </button>
                </div>
                {errors.code && (
                  <p key={`reg-mc-${shakeKey}`} className="text-[11px] font-medium text-destructive animate-shake">{errors.code}</p>
                )}
              </div>
            </>
          )}

          {/* 4. 密码输入字段 */}
          <div className="space-y-1.5 text-left">
            <label className="text-[13px] font-medium text-zinc-900 dark:text-zinc-100">
              {t('auth.password_placeholder')}
            </label>
            <div className="relative">
              <input
                type={showPassword ? 'text' : 'password'}
                placeholder={t('auth.password_placeholder')}
                value={passwordVal}
                onChange={(e) => {
                  setPasswordVal(e.target.value);
                  if (errors.password) setErrors(prev => ({ ...prev, password: '' }));
                }}
                className={`flex h-9 w-full rounded-md border bg-transparent pl-3 pr-10 py-1 text-sm shadow-xs transition-all duration-200 placeholder:text-zinc-400/70 dark:placeholder:text-zinc-600/70 focus-visible:outline-none focus-visible:ring-1 disabled:cursor-not-allowed disabled:opacity-50
                  ${errors.password ? 'border-destructive focus-visible:ring-destructive focus-visible:border-destructive' : 'border-zinc-200 dark:border-zinc-800 focus-visible:border-zinc-950 dark:focus-visible:border-zinc-300 focus-visible:ring-zinc-950/10 dark:focus-visible:ring-zinc-300/10'}`}
              />
              <button
                type="button"
                onClick={() => setShowPassword(!showPassword)}
                className="absolute right-1.5 top-1.5 w-6.5 h-6.5 rounded flex items-center justify-center text-zinc-400 hover:text-zinc-900 dark:hover:text-zinc-100 hover:bg-zinc-100 dark:hover:bg-zinc-900 transition-all cursor-pointer"
              >
                {showPassword ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
              </button>
            </div>
            {errors.password && (
              <p key={`reg-p-${shakeKey}`} className="text-[11px] font-medium text-destructive animate-shake">{errors.password}</p>
            )}
          </div>

          {/* 5. 确认密码输入字段 */}
          <div className="space-y-1.5 text-left">
            <label className="text-[13px] font-medium text-zinc-900 dark:text-zinc-100">
              {t('auth.confirm_password_placeholder')}
            </label>
            <div className="relative">
              <input
                type={showConfirmPassword ? 'text' : 'password'}
                placeholder={t('auth.confirm_password_placeholder')}
                value={confirmPasswordVal}
                onChange={(e) => {
                  setConfirmPasswordVal(e.target.value);
                  if (errors.confirm_password) setErrors(prev => ({ ...prev, confirm_password: '' }));
                }}
                className={`flex h-9 w-full rounded-md border bg-transparent pl-3 pr-10 py-1 text-sm shadow-xs transition-all duration-200 placeholder:text-zinc-400/70 dark:placeholder:text-zinc-600/70 focus-visible:outline-none focus-visible:ring-1 disabled:cursor-not-allowed disabled:opacity-50
                  ${errors.confirm_password ? 'border-destructive focus-visible:ring-destructive focus-visible:border-destructive' : 'border-zinc-200 dark:border-zinc-800 focus-visible:border-zinc-950 dark:focus-visible:border-zinc-300 focus-visible:ring-zinc-950/10 dark:focus-visible:ring-zinc-300/10'}`}
              />
              <button
                type="button"
                onClick={() => setShowConfirmPassword(!showConfirmPassword)}
                className="absolute right-1.5 top-1.5 w-6.5 h-6.5 rounded flex items-center justify-center text-zinc-400 hover:text-zinc-900 dark:hover:text-zinc-100 hover:bg-zinc-100 dark:hover:bg-zinc-900 transition-all cursor-pointer"
              >
                {showConfirmPassword ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
              </button>
            </div>
            {errors.confirm_password && (
              <p key={`reg-cp-${shakeKey}`} className="text-[11px] font-medium text-destructive animate-shake">{errors.confirm_password}</p>
            )}
          </div>

          {/* 注册同意协议勾选框 */}
          {(Boolean(settings?.agreement?.tos_enabled) || Boolean(settings?.agreement?.privacy_enabled)) && (
            <div className="space-y-1 text-left">
              <label className="flex items-start gap-2 text-[12px] text-zinc-500 dark:text-zinc-400 select-none cursor-pointer">
                <input
                  type="checkbox"
                  checked={agreed}
                  onChange={(e) => {
                    setAgreed(e.target.checked);
                    if (errors.agree) setErrors(prev => ({ ...prev, agree: '' }));
                  }}
                  className="mt-0.5 h-3.5 w-3.5 rounded border-zinc-300 text-zinc-900 focus:ring-zinc-900/10 dark:border-zinc-800 dark:bg-zinc-900 dark:focus:ring-zinc-300/10 cursor-pointer"
                />
                <span className="leading-snug">
                  {i18n.language.startsWith('zh') ? '我已阅读并同意我们的' : 'I have read and agree to our '}
                  {settings?.agreement?.tos_enabled === true && (
                    <a href={getAgreementUrl('tos')} target="_blank" rel="noopener noreferrer" className="!text-zinc-700 dark:!text-zinc-300 hover:!text-zinc-950 dark:hover:!text-zinc-50 hover:underline font-semibold transition-colors">
                      {i18n.language.startsWith('zh') ? '《服务条款》' : 'Terms of Service'}
                    </a>
                  )}
                  {settings?.agreement?.tos_enabled === true && settings?.agreement?.privacy_enabled === true && (
                    <span> {i18n.language.startsWith('zh') ? '和' : 'and'} </span>
                  )}
                  {settings?.agreement?.privacy_enabled === true && (
                    <a href={getAgreementUrl('privacy')} target="_blank" rel="noopener noreferrer" className="!text-zinc-700 dark:!text-zinc-300 hover:!text-zinc-950 dark:hover:!text-zinc-50 hover:underline font-semibold transition-colors">
                      {i18n.language.startsWith('zh') ? '《隐私协议》' : 'Privacy Policy'}
                    </a>
                  )}
                </span>
              </label>
              {errors.agree && (
                <p key={`reg-ag-${shakeKey}`} className="text-[11px] font-medium text-destructive animate-shake pl-5.5">{errors.agree}</p>
              )}
            </div>
          )}

          {formError && (
            <p key={`form-${shakeKey}`} className="text-[12px] font-medium text-destructive text-center animate-shake">{formError}</p>
          )}

          {/* 极致黑白灰反转注册按钮 */}
          <button
            type="submit"
            disabled={loading}
            className="inline-flex items-center justify-center rounded-md text-sm font-semibold transition-all duration-200 focus-visible:outline-none focus-visible:ring-1 disabled:pointer-events-none disabled:opacity-50 bg-zinc-900 text-zinc-50 hover:bg-zinc-900/90 dark:bg-zinc-50 dark:text-zinc-900 dark:hover:bg-zinc-50/90 shadow-sm h-9 px-4 py-2 w-full cursor-pointer"
          >
            {loading && (
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            )}
            {t('auth.register_btn')}
          </button>

        </form>
      )}
    </AuthLayout>
    </SignedInEntryGate>
  );
};

export default Register;
