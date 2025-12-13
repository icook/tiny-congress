/**
 * RFC 8785 JSON Canonicalization Scheme (JCS)
 * Produces deterministic JSON serialization for cryptographic operations
 */

/**
 * Canonicalize a JSON value per RFC 8785
 *
 * Rules:
 * - Objects: keys sorted lexicographically, no whitespace
 * - Arrays: preserve order, no whitespace
 * - Strings: minimal escaping (only control chars, quotes, backslash)
 * - Numbers: preserve original format (no scientific notation conversion)
 * - Booleans/null: lowercase
 *
 * @param value - JSON-serializable value
 * @returns Canonical JSON string
 */
export function canonicalize(value: unknown): string {
  if (value === null) {
    return 'null';
  }

  if (typeof value === 'boolean') {
    return value ? 'true' : 'false';
  }

  if (typeof value === 'number') {
    if (!Number.isFinite(value)) {
      throw new Error('Cannot canonicalize non-finite number');
    }
    // Use original number representation
    return String(value);
  }

  if (typeof value === 'string') {
    return canonicalizeString(value);
  }

  if (Array.isArray(value)) {
    const elements = value.map((v) => canonicalize(v));
    return `[${elements.join(',')}]`;
  }

  if (typeof value === 'object') {
    const obj = value as Record<string, unknown>;
    const keys = Object.keys(obj).sort();
    const pairs = keys.map((key) => `${canonicalizeString(key)}:${canonicalize(obj[key])}`);
    return `{${pairs.join(',')}}`;
  }

  throw new Error(`Cannot canonicalize type: ${typeof value}`);
}

/**
 * Canonicalize a string with minimal escaping
 *
 * @param str - String to escape
 * @returns JSON string with quotes and escapes
 */
function canonicalizeString(str: string): string {
  let result = '"';

  for (let i = 0; i < str.length; i++) {
    const char = str[i];
    const code = str.charCodeAt(i);

    // Control characters (U+0000 to U+001F)
    if (code < 0x20) {
      switch (char) {
        case '\b':
          result += '\\b';
          break;
        case '\f':
          result += '\\f';
          break;
        case '\n':
          result += '\\n';
          break;
        case '\r':
          result += '\\r';
          break;
        case '\t':
          result += '\\t';
          break;
        default:
          // Use \uXXXX for other control characters
          result += `\\u${code.toString(16).padStart(4, '0')}`;
      }
    } else if (char === '"') {
      result += '\\"';
    } else if (char === '\\') {
      result += '\\\\';
    } else {
      result += char;
    }
  }

  result += '"';
  return result;
}

/**
 * Canonicalize and encode to UTF-8 bytes
 *
 * @param value - JSON-serializable value
 * @returns UTF-8 encoded canonical JSON
 */
export function canonicalizeToBytes(value: unknown): Uint8Array {
  const canonical = canonicalize(value);
  return new TextEncoder().encode(canonical);
}
