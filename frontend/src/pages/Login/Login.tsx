/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useState, useEffect } from 'react';
import { useNavigate, Link, useSearchParams } from 'react-router-dom';
import { message } from 'antd';
import { 
  User, 
  Mail, 
  Phone, 
  Lock, 
  Eye, 
  EyeOff, 
  Loader2 
} from 'lucide-react';
import { WechatOutlined, GoogleOutlined } from '@ant-design/icons';
import request from '../../utils/request';
import { apiErrMsg } from '../../utils/apiErr';
import { buildRegisterPath, resolveInviteParams } from '../../utils/inviteTracking';
import { useTranslation } from 'react-i18next';
import useAuthStore from '../../store/auth';
import useSettingsStore from '../../store/settings';
import AuthLayout from '../../layouts/AuthLayout';
import SignedInEntryGate from './SignedInEntryGate';
import type { AuthMethodOption } from '../../layouts/AuthLayout';
import WechatQR from '../../components/WechatQR';
import GoogleIcon from '../../components/GoogleIcon';
import { sanitizeRedirectPath, stripAuthParamsFromUrl } from '../../utils/safeRedirect';
import { playground2026LoginLanding } from '../../utils/siteDefaultLanding';
import { browserTimezone } from '../../utils/timedisplay';
import AccountPickModal from './AccountPickModal';

const Login: React.FC = () => {
  const { t, i18n } = useTranslation();
  const [loading, setLoading] = useState(false);
  const [activeTab, setActiveTab] = useState('username');
  
  // 表单状态值
  const [usernameVal, setUsernameVal] = useState('');
  const [passwordVal, setPasswordVal] = useState('');
  const [showPassword, setShowPassword] = useState(false);
  const [wechatState, setWechatState] = useState('');
  
  // 校验错误状态
  const [errors, setErrors] = useState<{ username?: string; password?: string }>({});
  const [formError, setFormError] = useState('');
  // 用于触发再次抖动的 key 状态
  const [shakeKey, setShakeKey] = useState(0);
  const [selectOpen, setSelectOpen] = useState(false);
  const [candidates, setCandidates] = useState<{ uid: string; username: string; nickname?: string | null }[]>([]);
  const [pendingOAuthCode, setPendingOAuthCode] = useState('');

  const navigate = useNavigate();
  const { token, user, setToken, setUser } = useAuthStore();
  const { settings, fetchSettings } = useSettingsStore();
  const [searchParams] = useSearchParams();

  useEffect(() => {
    const prefill = searchParams.get('identifier') || searchParams.get('email') || searchParams.get('mobile') || '';
    const type = searchParams.get('type') || '';
    if (prefill) setUsernameVal(prefill);
    if (type === 'email' || type === 'mobile' || type === 'username' || type === 'wechat') {
      setActiveTab(type);
    }
  }, [searchParams]);

  useEffect(() => { 
    if (!settings) fetchSettings(); 
  }, []);

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

  // OAuth 回调或管理员代入一键登录
  useEffect(() => {
    const code = searchParams.get('code');
    const legacyToken = searchParams.get('token');
    const handoffKey = searchParams.get('handoff');
    const isImpersonate = searchParams.get('impersonate') === '1';
    const explicitRedirect = searchParams.get('redirect');

    const goAfterAuth = async (profile?: { role?: string; user_group?: string; level_id?: string | number | null }) => {
      if (isImpersonate) {
        const redirectTo = sanitizeRedirectPath(explicitRedirect || '/dashboard');
        window.location.replace(redirectTo.startsWith('/') ? redirectTo : '/dashboard');
        return;
      }
      if (explicitRedirect) {
        navigate(sanitizeRedirectPath(explicitRedirect), { replace: true });
        return;
      }
      if (profile?.role !== 'admin') {
        const landing = await playground2026LoginLanding(profile);
        if (landing) {
          navigate(landing, { replace: true });
          return;
        }
      }
      navigate('/dashboard', { replace: true });
    };

    const finishWithToken = (tok: string) => {
      setToken(tok, isImpersonate);
      stripAuthParamsFromUrl();
      request.get('/user/profile')
        .then((res: any) => {
          setUser(res, isImpersonate);
          void goAfterAuth(res);
        })
        .catch((e) => {
          console.error('Auto login failed:', e);
          void goAfterAuth();
        });
    };

    if (handoffKey) {
      const tok = localStorage.getItem(handoffKey);
      localStorage.removeItem(handoffKey);
      if (tok) {
        finishWithToken(tok);
        return;
      }
    }

    if (code) {
      request
        .post('/auth/exchange-login-code', { code }, { skipErrorHandler: true } as any)
        .then((res: any) => {
          if (res?.need_select && Array.isArray(res.candidates) && res.candidates.length > 1) {
            stripAuthParamsFromUrl();
            setPendingOAuthCode(code);
            setCandidates(res.candidates);
            setSelectOpen(true);
            return;
          }
          const tok = res?.token || res?.data?.token;
          if (tok) finishWithToken(tok);
          else stripAuthParamsFromUrl();
        })
        .catch((e) => {
          console.error('Exchange login code failed:', e);
          stripAuthParamsFromUrl();
          setFormError(toFormError(e));
        });
      return;
    }

    if (legacyToken) {
      // 兼容旧链接；立刻从地址栏清除
      finishWithToken(legacyToken);
    }
  }, [searchParams, setToken, setUser, navigate]);

  const baseTitle = settings?.site?.login_title || settings?.site?.name || 'Tkeapi';
  const loginTitle = `${baseTitle} ${t('login.title')}`;
  const loginTitleUrl = settings?.site?.login_title_url || '';
  const loginSubtitle = settings?.site?.login_subtitle || 'Next-gen LLM API Gateway';
  const login = settings?.login;
  const reg = settings?.registration;

  // 渲染可用登录 tabs
  const loginTabs: { key: string; label: string; icon: React.ReactNode; placeholder: string }[] = [];
  if (!login || reg?.enable_username_registration || login.enable_username_login) {
    loginTabs.push({ 
      key: 'username', 
      label: t('login.username_login'), 
      icon: <User className="w-3.5 h-3.5" />, 
      placeholder: t('login.username') 
    });
  }
  if (login?.enable_mobile_login) {
    loginTabs.push({ 
      key: 'mobile', 
      label: t('login.mobile_login'), 
      icon: <Phone className="w-3.5 h-3.5" />, 
      placeholder: t('auth.mobile_placeholder') 
    });
  }
  if (login?.enable_email_login) {
    loginTabs.push({ 
      key: 'email', 
      label: t('login.email_login'), 
      icon: <Mail className="w-3.5 h-3.5" />, 
      placeholder: t('auth.email_placeholder') 
    });
  }

  // 默认 Tab 设置
  useEffect(() => {
    if (activeTab === 'wechat' && login?.enable_wechat_login) return;
    if (loginTabs.length > 0 && !loginTabs.some(tab => tab.key === activeTab)) {
      setActiveTab(loginTabs[0].key);
    } else if (loginTabs.length === 0 && login?.enable_wechat_login) {
      setActiveTab('wechat');
    }
  }, [settings]);

  const currentTab = loginTabs.find(tab => tab.key === activeTab) || loginTabs[0];

  const handleTabChange = (key: string) => {
    setActiveTab(key);
    setErrors({});
    setFormError('');
    setSelectOpen(false);
    setCandidates([]);
    setPendingOAuthCode('');
  };

  const toFormError = (error: unknown) => {
    const msg = apiErrMsg(error, t('common.error'));
    return msg === 'Account disabled' ? t('login.account_disabled') : msg;
  };

  const handleLogin = async (e: React.FormEvent) => {
    e.preventDefault();
    if (loading) return;

    // 前端基础校验
    const tempErrors: { username?: string; password?: string } = {};
    if (!usernameVal.trim()) {
      tempErrors.username = currentTab?.placeholder || t('login.username');
    }
    if (!passwordVal) {
      tempErrors.password = t('login.password');
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
    message.destroy();

    try {
      const res = await (request.post('/auth/login', { 
        username: usernameVal.trim(), 
        password: passwordVal, 
        login_type: activeTab 
      }, { skipErrorHandler: true } as any) as any);
      if (res?.need_select && Array.isArray(res.candidates) && res.candidates.length > 1) {
        setCandidates(res.candidates);
        setSelectOpen(true);
        setLoading(false);
        return;
      }
      finishLogin(res);
    } catch (error) {
      setFormError(toFormError(error));
      setShakeKey(k => k + 1);
    } finally {
      setTimeout(() => setLoading(false), 800);
    }
  };

  const finishLogin = async (res: any) => {
    setSelectOpen(false);
    setToken(res.token); 
    setUser(res.user);
    if (res.user?.role === 'admin') {
      const adminPath = localStorage.getItem('tokensbyte_admin_path') || 'admin1688';
      navigate(`/${adminPath}/dashboard`);
      return;
    }
    const explicit = searchParams.get('redirect');
    if (explicit) {
      navigate(sanitizeRedirectPath(explicit));
      return;
    }
    const landing = await playground2026LoginLanding(res.user);
    navigate(landing || '/dashboard');
  };

  const handleSelectAccount = async (uid: string) => {
    if (loading) return;
    setLoading(true);
    try {
      const res = pendingOAuthCode
        ? await (request.post('/auth/exchange-login-code', {
            code: pendingOAuthCode,
            uid,
          }, { skipErrorHandler: true } as any) as any)
        : await (request.post('/auth/login', {
            username: usernameVal.trim(),
            password: passwordVal,
            login_type: activeTab,
            uid,
          }, { skipErrorHandler: true } as any) as any);
      setPendingOAuthCode('');
      finishLogin(res);
    } catch (error) {
      setFormError(toFormError(error));
      setShakeKey(k => k + 1);
    } finally {
      setTimeout(() => setLoading(false), 800);
    }
  };

  // 生成传递给 AuthLayout 的底部快捷渠道
  const layoutMethods: AuthMethodOption[] = [];
  if (login?.enable_wechat_login) {
    layoutMethods.push({ 
      key: 'wechat', 
      label: t('login.wechat_login'), 
      icon: <WechatOutlined style={{ fontSize: '18px', color: '#07C160' }} /> 
    });
  }
  if (login?.enable_google_login) {
    layoutMethods.push({ 
      key: 'google', 
      label: t('login.google_login'), 
      icon: <GoogleIcon className="w-4 h-4" />,
      onClick: () => {
        const { aff, team } = resolveInviteParams();
        const params = new URLSearchParams();
        if (aff) params.set('aff', aff);
        if (team) params.set('team', team);
        const tz = browserTimezone();
        if (tz) params.set('timezone', tz);
        const qs = params.toString();
        window.location.href = `/api/v1/auth/oauth/google${qs ? `?${qs}` : ''}`;
      },
    });
  }
  // 将所有的表单通道以图标形式追加入 layoutMethods
  loginTabs.forEach(tab => {
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

  const wechatAppId = settings?.wechat_oauth_app_id || '';
  const wechatBaseOrigin = settings?.wechat_oauth_callback_domain || window.location.origin;
  const wechatRedirectUri = `${wechatBaseOrigin}/api/v1/auth/oauth/wechat/callback`;

  // 从后端获取 HMAC 签名的 OAuth state（嵌入 3 天内邀请参数与当前 origin，禁止前端自造）
  useEffect(() => {
    if (!settings?.login?.enable_wechat_login) return;
    let cancelled = false;
    const { aff, team } = resolveInviteParams();
    const params: Record<string, string> = { 
      provider: 'wechat',
      origin: window.location.origin,
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
  }, [settings?.login?.enable_wechat_login, settings?.wechat_oauth_callback_domain]);

  // 邀请码处理：登录页「去注册」带回 3 天内未清缓存的 aff/team
  const getStoredInviteParam = () => buildRegisterPath();

  const bottomLinks = (
    <div className="flex items-center justify-center gap-1.5 text-xs text-zinc-500 dark:text-zinc-400">
      <span>{i18n.language.startsWith('zh') ? '没有账号？' : "Don't have an account?"}</span>
      <Link to={getStoredInviteParam()} className="!text-zinc-900 dark:!text-zinc-100 hover:underline font-semibold transition-colors">
        {t('auth.register_link')}
      </Link>
    </div>
  );

  const completingExternalAuth = Boolean(
    searchParams.get('code') || searchParams.get('token') || searchParams.get('handoff'),
  );

  return (
    <SignedInEntryGate enabled={!completingExternalAuth}>
    <AuthLayout
      title={loginTitle}
      subtitle={loginSubtitle}
      titleHref={loginTitleUrl}
      logo={settings?.site?.logo}
      loading={!settings}
      methodsLabel={t('login.title')}
      methods={layoutMethods}
      activeMethod={activeTab}
      onMethodChange={handleTabChange}
      bottomLinks={bottomLinks}
    >
      {selectOpen ? (
        <AccountPickModal
          title={t('auth.select_account_title')}
          hint={t('auth.select_account_hint')}
          loading={loading}
          error={formError}
          backLabel={t('common.back')}
          candidates={candidates}
          onSelect={handleSelectAccount}
          onBack={() => {
            setSelectOpen(false);
            setPendingOAuthCode('');
            setFormError('');
          }}
        />
      ) : activeTab === 'wechat' ? (
        <div className="flex flex-col items-center justify-center p-2 bg-background border border-zinc-200 dark:border-zinc-800 rounded-lg shadow-xs">
          {wechatState ? (
            <WechatQR appId={wechatAppId} redirectUri={wechatRedirectUri} state={wechatState} />
          ) : (
            <div className="py-10 text-zinc-500 text-sm">加载中...</div>
          )}
          {loginTabs.length > 0 && (
            <button 
              type="button" 
              onClick={() => handleTabChange(loginTabs[0]?.key || 'username')}
              className="text-xs text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200 mt-2 transition-colors cursor-pointer"
            >
              {t('auth.back_to_login')}
            </button>
          )}
        </div>
      ) : (
        <form onSubmit={handleLogin} className="space-y-4">

          {/* 账号输入框 */}
          <div className="space-y-1.5 text-left">
            <label className="text-[13px] font-medium text-zinc-900 dark:text-zinc-100">
              {currentTab?.label || t('login.username')}
            </label>
            <input
              type={activeTab === 'email' ? 'email' : activeTab === 'mobile' ? 'tel' : 'text'}
              placeholder={currentTab?.placeholder || t('login.username')}
              value={usernameVal}
              onChange={(e) => {
                setUsernameVal(e.target.value);
                if (errors.username) setErrors(prev => ({ ...prev, username: undefined }));
                if (formError) setFormError('');
              }}
              className={`flex h-9 w-full rounded-md border bg-transparent px-3 py-1 text-sm shadow-xs transition-all duration-200 placeholder:text-zinc-400/70 dark:placeholder:text-zinc-600/70 focus-visible:outline-none focus-visible:ring-1 disabled:cursor-not-allowed disabled:opacity-50
                ${errors.username 
                  ? 'border-destructive focus-visible:ring-destructive focus-visible:border-destructive' 
                  : 'border-zinc-200 dark:border-zinc-800 focus-visible:border-zinc-950 dark:focus-visible:border-zinc-300 focus-visible:ring-zinc-950/10 dark:focus-visible:ring-zinc-300/10'
                }`}
            />
            {errors.username && (
              <p key={`u-${shakeKey}`} className="text-[11px] font-medium text-destructive animate-shake">{errors.username}</p>
            )}
          </div>

          {/* 密码输入框 */}
          <div className="space-y-1.5 text-left">
            <div className="flex items-center justify-between">
              <label className="text-[13px] font-medium text-zinc-900 dark:text-zinc-100">
                {t('login.password')}
              </label>
              <Link to="/forgot-password" className="!text-zinc-500 dark:!text-zinc-400 hover:!text-zinc-900 dark:hover:!text-zinc-100 hover:underline text-[12px] font-medium transition-colors">
                {t('auth.forgot_password_link')}
              </Link>
            </div>
            <div className="relative">
              <input
                type={showPassword ? 'text' : 'password'}
                placeholder={t('login.password')}
                value={passwordVal}
                onChange={(e) => {
                  setPasswordVal(e.target.value);
                  if (errors.password) setErrors(prev => ({ ...prev, password: undefined }));
                  if (formError) setFormError('');
                }}
                className={`flex h-9 w-full rounded-md border bg-transparent pl-3 pr-10 py-1 text-sm shadow-xs transition-all duration-200 placeholder:text-zinc-400/70 dark:placeholder:text-zinc-600/70 focus-visible:outline-none focus-visible:ring-1 disabled:cursor-not-allowed disabled:opacity-50
                  ${errors.password 
                    ? 'border-destructive focus-visible:ring-destructive focus-visible:border-destructive' 
                    : 'border-zinc-200 dark:border-zinc-800 focus-visible:border-zinc-950 dark:focus-visible:border-zinc-300 focus-visible:ring-zinc-950/10 dark:focus-visible:ring-zinc-300/10'
                  }`}
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
              <p key={`p-${shakeKey}`} className="text-[11px] font-medium text-destructive animate-shake">{errors.password}</p>
            )}
          </div>

          {formError && (
            <p key={`form-${shakeKey}`} className="text-[12px] font-medium text-destructive text-center animate-shake">{formError}</p>
          )}

          {/* 极致黑白灰反转登录按钮 */}
          <button
            type="submit"
            disabled={loading}
            className="inline-flex items-center justify-center rounded-md text-sm font-semibold transition-all duration-200 focus-visible:outline-none focus-visible:ring-1 disabled:pointer-events-none disabled:opacity-50 bg-zinc-900 text-zinc-50 hover:bg-zinc-900/90 dark:bg-zinc-50 dark:text-zinc-900 dark:hover:bg-zinc-50/90 shadow-sm h-9 px-4 py-2 w-full cursor-pointer"
          >
            {loading && (
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            )}
            {t('login.sign_in')}
          </button>

          {/* 登录同意站点协议说明 */}
          {(Boolean(settings?.agreement?.tos_enabled) || Boolean(settings?.agreement?.privacy_enabled)) && (
            <p className="text-[11px] text-zinc-400 dark:text-zinc-500 text-center leading-normal mt-2 select-none">
              {i18n.language.startsWith('zh') ? '登录即代表您已同意我们的' : 'By signing in, you agree to our '}
              {settings?.agreement?.tos_enabled === true && (
                <a href={getAgreementUrl('tos')} target="_blank" rel="noopener noreferrer" className="!text-zinc-500 dark:!text-zinc-400 hover:!text-zinc-900 dark:hover:!text-zinc-100 hover:underline font-medium transition-colors">
                  {i18n.language.startsWith('zh') ? '《服务条款》' : 'Terms of Service'}
                </a>
              )}
              {settings?.agreement?.tos_enabled === true && settings?.agreement?.privacy_enabled === true && (
                <span> {i18n.language.startsWith('zh') ? '和' : 'and'} </span>
              )}
              {settings?.agreement?.privacy_enabled === true && (
                <a href={getAgreementUrl('privacy')} target="_blank" rel="noopener noreferrer" className="!text-zinc-500 dark:!text-zinc-400 hover:!text-zinc-900 dark:hover:!text-zinc-100 hover:underline font-medium transition-colors">
                  {i18n.language.startsWith('zh') ? '《隐私协议》' : 'Privacy Policy'}
                </a>
              )}
            </p>
          )}

        </form>
      )}
    </AuthLayout>
    </SignedInEntryGate>
  );
};

export default Login;
