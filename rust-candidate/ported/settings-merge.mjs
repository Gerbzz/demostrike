// Mirrors the upstream update-settings deep merge, including its option to
// replace filter arrays. A partial screen update must not erase the profile.
export function mergeSettings(current, partial, preserveSourceArray = false) {
  if (Array.isArray(partial)) {
    return preserveSourceArray ? [...partial] : [...(Array.isArray(current) ? current : []), ...partial];
  }
  if (partial !== null && typeof partial === 'object') {
    const result = { ...(current !== null && typeof current === 'object' ? current : {}) };
    for (const [key, value] of Object.entries(partial)) {
      result[key] = mergeSettings(result[key], value, preserveSourceArray);
    }
    return result;
  }
  return partial;
}
