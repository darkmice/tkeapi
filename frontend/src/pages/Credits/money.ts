/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

export function formatCreditMoney(symbol: string, value: number, signed = false) {
  const n = Number(value);
  const safe = Number.isFinite(n) ? n : 0;
  const text = `${symbol}${Math.abs(safe).toFixed(2)}`;
  if (!signed) return safe < 0 ? `-${text}` : text;
  if (safe > 0) return `+${text}`;
  if (safe < 0) return `-${text}`;
  return text;
}
