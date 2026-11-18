<!--
	The crate cards on the home page; a crate that has not been carved out yet is marked planned.

	(c) Copyright 2026 Liminal HQ, Scott Morris
	SPDX-License-Identifier: Apache-2.0 OR MIT
-->
<script setup lang="ts">
import { withBase } from 'vitepress';

interface Crate {
	name: string;
	summary: string;
	guide?: string;
	api?: string;
}

const crates: Crate[] = [
	{
		name: 'oplstream',
		summary: 'The reference implementation of OPX: reader, writer, stepper, validator, text twin and IMF, DRO and VGM import. The core is no_std.',
		guide: '/guide/oplstream',
		api: '/api/oplstream/index.html'
	},
	{
		name: 'undertone-mini',
		summary: "Undertone's mini-notation in Rust: the parser, pattern combinators and exact fraction time.",
		guide: '/guide/undertone-mini',
		api: '/api/undertone_mini/index.html'
	},
	{
		name: 'undertone-dsp',
		summary: "Undertone's voices, Web Audio graph and in-house renderer, for native and WebAssembly.",
		guide: '/guide/undertone-dsp',
		api: '/api/undertone_dsp/index.html'
	},
	{
		name: 'undertone-opl',
		summary: 'OPL2 and OPL3 sequencing and channel allocation. Waiting for the code to move in from the studio.'
	},
	{
		name: 'undertone-sound',
		summary: 'The song model and compiler. Waiting for the code to move in from the studio.'
	}
];
</script>

<template>
	<div class="crate-cards">
		<article v-for="crate in crates" :key="crate.name" class="crate-card" :class="{ planned: !crate.guide }">
			<header>
				<h3>{{ crate.name }}</h3>
				<span v-if="!crate.guide" class="badge">planned</span>
			</header>
			<p>{{ crate.summary }}</p>
			<footer v-if="crate.guide">
				<a :href="withBase(crate.guide)">Guide</a>
				<a :href="withBase(crate.api!)">API</a>
			</footer>
		</article>
	</div>
</template>

<style scoped>
.crate-cards {
	display: grid;
	grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
	gap: 16px;
	max-width: 1152px;
	margin: 0 auto;
	padding: 24px;
}

.crate-card {
	display: flex;
	flex-direction: column;
	gap: 8px;
	padding: 20px;
	border: 1px solid var(--vp-c-divider);
	border-radius: 12px;
	background: var(--vp-c-bg-soft);
}

.crate-card.planned {
	border-style: dashed;
	opacity: 0.8;
}

.crate-card header {
	display: flex;
	align-items: center;
	justify-content: space-between;
	gap: 8px;
}

.crate-card h3 {
	margin: 0;
	font-family: var(--vp-font-family-mono);
	font-size: 16px;
	color: var(--vp-c-brand-1);
}

.badge {
	padding: 1px 8px;
	border: 1px solid var(--vp-c-indigo-1);
	border-radius: 999px;
	font-size: 12px;
	color: var(--vp-c-indigo-1);
}

.crate-card p {
	flex: 1;
	margin: 0;
	font-size: 14px;
	line-height: 1.6;
	color: var(--vp-c-text-2);
}

.crate-card footer {
	display: flex;
	gap: 16px;
	font-size: 14px;
	font-weight: 500;
}
</style>
