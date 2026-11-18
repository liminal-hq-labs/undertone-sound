<!--
	The repository's animated hero banner. It is shown as an image, which keeps its SMIL motion and
	its reduced-motion rules.

	(c) Copyright 2026 Liminal HQ, Scott Morris
	SPDX-License-Identifier: Apache-2.0 OR MIT
-->
<script setup lang="ts">
import heroUrl from '../../../../assets/hero.svg?url';
</script>

<template>
	<div class="hero-banner">
		<img :src="heroUrl" alt="Undertone Sound — Shared Rust sound core for Undertone" width="1200" height="320" />
	</div>
</template>

<style scoped>
.hero-banner {
	max-width: 1152px;
	margin: 0 auto;
	padding: 24px 24px 0;
}

.hero-banner img {
	display: block;
	width: 100%;
	height: auto;
	border-radius: 12px;
}

@media (min-width: 640px) {
	.hero-banner {
		padding: 32px 48px 0;
	}
}
</style>
