/**
 * Attempt to derive a reasonable device name from the browser's user agent.
 */
export function getDeviceName(): string {
  const ua = navigator.userAgent;
  if (ua.includes('iPhone') || ua.includes('iPad') || ua.includes('iPod')) {
    return 'iOS Device';
  }
  if (ua.includes('Android')) {
    return 'Android Device';
  }
  if (ua.includes('Mac')) {
    return 'Mac';
  }
  if (ua.includes('Windows')) {
    return 'Windows PC';
  }
  if (ua.includes('Linux')) {
    return 'Linux';
  }
  return 'Browser';
}
