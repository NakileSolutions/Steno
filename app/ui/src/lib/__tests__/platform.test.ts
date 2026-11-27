import { describe, expect, it } from 'vitest';
import { detectPlatform, isLinux, isMac, platform } from '../platform';

describe('plateforme', () => {
  it('lit le système dans l’agent utilisateur', () => {
    expect(detectPlatform('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)')).toBe('macos');
    expect(detectPlatform('Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15')).toBe('linux');
    expect(detectPlatform('Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0')).toBe('linux');
    expect(detectPlatform('Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/126 Mobile')).toBe('other');
    expect(detectPlatform('Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/126')).toBe('other');
  });
  it('accepte une valeur forcée valide (?platform=…)', () => {
    expect(detectPlatform('Mozilla/5.0 (Windows NT 10.0)', 'linux')).toBe('linux');
    expect(detectPlatform('Mozilla/5.0 (X11; Linux x86_64)', 'macos')).toBe('macos');
    expect(detectPlatform('Mozilla/5.0 (X11; Linux x86_64)', 'windows')).toBe('linux');
  });
  it('tests unitaires : macOS (agent utilisateur de vitest.config.ts)', () => {
    expect(platform).toBe('macos');
    expect(isMac).toBe(true);
    expect(isLinux).toBe(false);
  });
});
