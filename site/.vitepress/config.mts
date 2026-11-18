// VitePress configuration for the Undertone Sound documentation site.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { defineConfig } from 'vitepress';
import type { UserConfig } from 'vitepress';

const base = '/undertone-sound/';

// The OPX specification, the behaviour specification and the roadmap are rendered from the
// repository's own `docs/` and `SPEC.md`, which link to each other as files (they read on GitHub
// too). The site serves the same documents as pages, so a link to one of those files becomes a
// link to its page.
const documentPages: Record<string, string> = {
  'OPX.md': 'opx',
  'SPEC.md': 'spec',
  'ROADMAP.md': 'roadmap'
};

// The decisions file is a contributor record, not user documentation, so the site has no page for
// it. A link to it from a rendered document points at the file in the repository.
const decisionsUrl = 'https://github.com/liminal-hq-labs/undertone-sound/blob/main/docs/decisions.md';

type MarkdownConfig = NonNullable<NonNullable<UserConfig['markdown']>['config']>;

const documentLinks: MarkdownConfig = (md) => {
  md.core.ruler.push('document-links', (state) => {
    for (const block of state.tokens) {
      for (const token of block.children ?? []) {
        if (token.type !== 'link_open') continue;
        const href = token.attrGet('href');
        const match = href?.match(/^(?:\.{0,2}\/)*(?:docs\/)?([A-Za-z]+\.md)(#.*)?$/);
        if (match?.[1] === 'decisions.md') {
          token.attrSet('href', decisionsUrl);
          continue;
        }
        const page = match ? documentPages[match[1]] : undefined;
        if (page) token.attrSet('href', `/${page}${match?.[2] ?? ''}`);
      }
    }
  });
};

export default defineConfig({
  title: 'Undertone Sound',
  description: 'Shared Rust sound core for Undertone: OPX, the OPL register-stream format, and the Rust crates around it.',
  base,
  cleanUrls: true,
  lastUpdated: false,
  // The Rust API pages are produced by `cargo doc` and copied in beside the site by
  // `scripts/build-site.sh`, so VitePress does not know them.
  ignoreDeadLinks: [/^\/undertone-sound\/api\//, /^\/api\//],
  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: `${base}favicon.svg` }],
    ['meta', { name: 'theme-color', content: '#080a12' }]
  ],
  appearance: 'dark',
  markdown: {
    config: documentLinks
  },
  vite: {
    build: { assetsInlineLimit: 0 },
    // The home page shows `assets/hero.svg` from the repository root.
    server: { fs: { allow: ['..'] } }
  },
  themeConfig: {
    nav: [
      { text: 'Guide', link: '/guide/' },
      { text: 'OPX', link: '/opx' },
      { text: 'Roadmap', link: '/roadmap' },
      { text: 'Demo', link: '/demo' },
      { text: 'API', link: '/api' }
    ],
    sidebar: [
      {
        text: 'Guide',
        items: [
          { text: 'Overview', link: '/guide/' },
          { text: 'oplstream', link: '/guide/oplstream' },
          { text: 'undertone-mini', link: '/guide/undertone-mini' },
          { text: 'undertone-dsp', link: '/guide/undertone-dsp' }
        ]
      },
      {
        text: 'Reference',
        items: [
          { text: 'Demo', link: '/demo' },
          { text: 'OPX specification', link: '/opx' },
          { text: 'Behaviour (SPEC)', link: '/spec' },
          { text: 'Rust API', link: '/api' }
        ]
      },
      {
        text: 'Project',
        items: [
          { text: 'Roadmap', link: '/roadmap' }
        ]
      }
    ],
    search: { provider: 'local' },
    socialLinks: [{ icon: 'github', link: 'https://github.com/liminal-hq-labs/undertone-sound' }],
    footer: {
      message: 'An experiment from Liminal HQ Labs. Released under the Apache-2.0 or MIT licence.',
      copyright: '© 2026 Liminal HQ, Scott Morris'
    }
  }
});
