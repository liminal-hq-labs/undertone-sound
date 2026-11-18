// The site theme: VitePress's default theme in Undertone Sound's colours, with the hero banner.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { Theme } from 'vitepress';
import DefaultTheme from 'vitepress/theme';
import Demo from './components/Demo.vue';
import CrateCards from './components/CrateCards.vue';
import HeroBanner from './components/HeroBanner.vue';
import Layout from './Layout.vue';
import './custom.css';

export default {
  extends: DefaultTheme,
  Layout,
  enhanceApp({ app }) {
    app.component('Demo', Demo);
    app.component('CrateCards', CrateCards);
    app.component('HeroBanner', HeroBanner);
  }
} satisfies Theme;
