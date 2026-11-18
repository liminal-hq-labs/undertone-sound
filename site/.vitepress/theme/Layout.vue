<!--
	The default layout with the animated banner above the home page's hero.

	(c) Copyright 2026 Liminal HQ, Scott Morris
	SPDX-License-Identifier: Apache-2.0 OR MIT
-->
<script setup lang="ts">
import DefaultTheme from 'vitepress/theme';
import HeroBanner from './components/HeroBanner.vue';

const { Layout } = DefaultTheme;
</script>

<template>
	<Layout>
		<template #home-hero-before>
			<HeroBanner />
		</template>
	</Layout>
</template>
