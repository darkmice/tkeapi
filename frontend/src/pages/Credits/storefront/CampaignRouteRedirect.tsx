/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useEffect, useState } from 'react';
import { useNavigate, useLocation, useParams } from 'react-router-dom';
import request from '../../../utils/request';
import { couponCampaignPath, offerReturnFromState, rememberedOfferReturn } from './path';
import '../credits.css';

const CampaignRouteRedirect: React.FC = () => {
  const { campaignId } = useParams<{ campaignId: string }>();
  const navigate = useNavigate();
  const location = useLocation();
  const [missing, setMissing] = useState(false);

  useEffect(() => {
    let dead = false;
    (request.get(`/plugins/marketing_membership/storefront/${campaignId}`) as Promise<{ id: number; template_key?: string }>)
      .then((res) => {
        if (dead) return;
        const from = offerReturnFromState(location.state) || rememberedOfferReturn();
        navigate(couponCampaignPath(res.template_key, res.id || campaignId || ''), {
          replace: true,
          state: from ? { from } : undefined,
        });
      })
      .catch(() => {
        if (!dead) setMissing(true);
      });
    return () => {
      dead = true;
    };
  }, [campaignId, navigate]);

  if (!missing) return null;
  return <div className="order-standalone"><div className="credits-note">活动不存在或未启用。</div></div>;
};

export default CampaignRouteRedirect;
