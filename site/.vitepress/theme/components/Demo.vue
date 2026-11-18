<!--
	The browser demo: import a song as OPX, show what it is, and play it through the OPL emulator.

	(c) Copyright 2026 Liminal HQ, Scott Morris
	SPDX-License-Identifier: Apache-2.0 OR MIT
-->
<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef } from 'vue';
import wasmUrl from '../wasm/undertone_wasm_bg.wasm?url';
import titleThemeUrl from '../../../../fixtures/opx/title-theme.imf?url';

type Wasm = typeof import('../wasm/undertone_wasm.js');
type Summary = import('../wasm/undertone_wasm.js').Summary;

interface Stats {
	chip: string;
	gridHz: number;
	totalTicks: number;
	seconds: number;
	writes: number;
	valid: boolean;
	error: string;
	hasLoop: boolean;
	streamLen: number;
	fileLen: number;
}

const wasm = shallowRef<Wasm | null>(null);
const source = ref<'title-theme' | 'file'>('title-theme');
const gridHz = ref(560);
const fileName = ref('');
const dragging = ref(false);
const view = ref<'header' | 'twin'>('header');
const stats = ref<Stats | null>(null);
const headerText = ref('');
const twinText = ref('');
const error = ref('');
const status = ref('Loading the WebAssembly module…');
const busy = ref(false);
const peak = ref<number | null>(null);
const playing = ref(false);
const waveform = ref('triangle');
const midi = ref(69);

let opx: Uint8Array | null = null;
let fileBytes: Uint8Array | null = null;
let context: AudioContext | null = null;
let playingSource: AudioBufferSourceNode | null = null;
let rendered: { opx: Uint8Array; buffer: AudioBuffer } | null = null;

const number = new Intl.NumberFormat('en-CA');

const rows = computed(() => {
	const s = stats.value;
	if (!s) return [];
	return [
		{ id: 'chip', label: 'Chip', value: s.chip },
		{ id: 'grid', label: 'Grid', value: `${number.format(s.gridHz)} Hz` },
		{ id: 'ticks', label: 'Ticks', value: number.format(s.totalTicks) },
		{ id: 'seconds', label: 'Length', value: `${s.seconds.toFixed(2)} s` },
		{ id: 'writes', label: 'Writes', value: number.format(s.writes) },
		{ id: 'valid', label: 'Valid', value: s.valid ? 'yes' : 'no' },
		{ id: 'bytes', label: 'OPX size', value: `${number.format(s.fileLen)} bytes` },
		{ id: 'loop', label: 'Loop point', value: s.hasLoop ? 'yes' : 'none' }
	];
});

function describe(thrown: unknown): string {
	return thrown instanceof Error ? thrown.message : String(thrown);
}

function toStats(summary: Summary): Stats {
	const result = {
		chip: summary.chip,
		gridHz: summary.grid_hz,
		totalTicks: summary.total_ticks,
		seconds: summary.seconds,
		writes: summary.writes,
		valid: summary.valid,
		error: summary.error,
		hasLoop: summary.has_loop,
		streamLen: summary.stream_len,
		fileLen: summary.file_len
	};
	summary.free();
	return result;
}

/** Imports the chosen source and fills the stats, the header table and the twin. */
function load(bytes: Uint8Array): void {
	const m = wasm.value;
	if (!m) return;
	stopPlayback();
	rendered = null;
	peak.value = null;
	error.value = '';
	try {
		const format = m.detectFormat(bytes);
		opx = m.importToOpx(bytes, format, gridHz.value);
		stats.value = toStats(m.summarize(opx));
		headerText.value = m.headerTable(opx);
		twinText.value = stats.value.valid ? m.textTwin(opx) : '';
		status.value = `Imported as ${format === 'opx' ? 'OPX (read as it is)' : format.toUpperCase()}.`;
		if (!stats.value.valid) error.value = `The stream is not valid: ${stats.value.error}`;
	} catch (thrown) {
		opx = null;
		stats.value = null;
		headerText.value = '';
		twinText.value = '';
		error.value = describe(thrown);
		status.value = 'Nothing loaded.';
	}
}

async function loadTitleTheme(): Promise<void> {
	busy.value = true;
	try {
		const response = await fetch(titleThemeUrl);
		if (!response.ok) throw new Error(`Title Theme could not be fetched (${response.status})`);
		fileBytes = new Uint8Array(await response.arrayBuffer());
		gridHz.value = 560;
		load(fileBytes);
	} catch (thrown) {
		error.value = describe(thrown);
	} finally {
		busy.value = false;
	}
}

async function useFile(file: File | undefined): Promise<void> {
	if (!file) return;
	source.value = 'file';
	fileName.value = file.name;
	fileBytes = new Uint8Array(await file.arrayBuffer());
	load(fileBytes);
}

function chooseTitleTheme(): void {
	source.value = 'title-theme';
	void loadTitleTheme();
}

function onFileInput(event: Event): void {
	void useFile((event.target as HTMLInputElement).files?.[0]);
}

function onDrop(event: DragEvent): void {
	dragging.value = false;
	void useFile(event.dataTransfer?.files?.[0]);
}

function reimport(): void {
	if (fileBytes) load(fileBytes);
}

function audio(): AudioContext {
	context ??= new AudioContext();
	return context;
}

function stopPlayback(): void {
	if (playingSource) {
		playingSource.onended = null;
		try {
			playingSource.stop();
		} catch {
			// It had already stopped.
		}
		playingSource.disconnect();
		playingSource = null;
	}
	playing.value = false;
}

function toBuffer(ctx: AudioContext, interleaved: Float32Array): AudioBuffer {
	const frames = interleaved.length / 2;
	const buffer = ctx.createBuffer(2, frames, ctx.sampleRate);
	const left = buffer.getChannelData(0);
	const right = buffer.getChannelData(1);
	for (let i = 0; i < frames; i++) {
		left[i] = interleaved[2 * i];
		right[i] = interleaved[2 * i + 1];
	}
	return buffer;
}

function peakOf(buffer: AudioBuffer): number {
	let max = 0;
	for (let c = 0; c < buffer.numberOfChannels; c++) {
		for (const sample of buffer.getChannelData(c)) max = Math.max(max, Math.abs(sample));
	}
	return max;
}

function start(ctx: AudioContext, buffer: AudioBuffer): void {
	stopPlayback();
	const node = ctx.createBufferSource();
	node.buffer = buffer;
	node.connect(ctx.destination);
	node.onended = () => {
		if (playingSource === node) stopPlayback();
	};
	playingSource = node;
	playing.value = true;
	node.start();
}

/** Renders the whole stream through the emulator in blocks, then plays it. */
async function play(): Promise<void> {
	const m = wasm.value;
	if (!m || !opx || busy.value) return;
	error.value = '';
	const ctx = audio();
	await ctx.resume();
	if (rendered && rendered.opx === opx) {
		start(ctx, rendered.buffer);
		return;
	}
	busy.value = true;
	try {
		const player = new m.Player(opx, ctx.sampleRate);
		const total = player.totalFrames() + Math.round(ctx.sampleRate / 2);
		const block = 16_384;
		const chunks: Float32Array[] = [];
		let done = 0;
		while (done < total) {
			const frames = Math.min(block, total - done);
			chunks.push(player.render(frames));
			done += frames;
			status.value = `Rendering… ${Math.round((100 * done) / total)}%`;
			// Let the page paint between blocks.
			await new Promise((resolve) => setTimeout(resolve, 0));
		}
		player.free();
		const interleaved = new Float32Array(total * 2);
		let at = 0;
		for (const chunk of chunks) {
			interleaved.set(chunk, at);
			at += chunk.length;
		}
		const buffer = toBuffer(ctx, interleaved);
		peak.value = peakOf(buffer);
		rendered = { opx, buffer };
		status.value = `Rendered ${buffer.duration.toFixed(1)} s through oplon. Peak ${peak.value.toFixed(3)}.`;
		start(ctx, buffer);
	} catch (thrown) {
		error.value = describe(thrown);
		status.value = 'Rendering failed.';
	} finally {
		busy.value = false;
	}
}

function playVoice(): void {
	const m = wasm.value;
	if (!m) return;
	error.value = '';
	try {
		const ctx = audio();
		void ctx.resume();
		const samples = m.renderVoice(waveform.value, midi.value, 1, ctx.sampleRate);
		const buffer = toBuffer(ctx, samples);
		status.value = `Rendered one ${waveform.value} voice through undertone-dsp. Peak ${peakOf(buffer).toFixed(3)}.`;
		start(ctx, buffer);
	} catch (thrown) {
		error.value = describe(thrown);
	}
}

onMounted(async () => {
	try {
		const m = await import('../wasm/undertone_wasm.js');
		await m.default({ module_or_path: wasmUrl });
		wasm.value = m;
		status.value = 'Ready.';
		await loadTitleTheme();
	} catch (thrown) {
		error.value = `The WebAssembly module could not start: ${describe(thrown)}`;
		status.value = 'Not ready.';
	}
});

onBeforeUnmount(() => {
	stopPlayback();
	void context?.close();
});
</script>

<template>
	<div class="demo" data-testid="demo">
		<section class="panel">
			<h2>Song</h2>
			<div class="choices" role="radiogroup" aria-label="Song">
				<label>
					<input type="radio" name="source" value="title-theme" :checked="source === 'title-theme'" @change="chooseTitleTheme" />
					Title Theme <span class="note">Fizz 1, IMF, 560 Hz</span>
				</label>
				<label>
					<input type="radio" name="source" value="file" :checked="source === 'file'" @change="source = 'file'" />
					Your own file
				</label>
			</div>
			<div
				class="drop"
				:class="{ over: dragging, active: source === 'file' }"
				@dragover.prevent="dragging = true"
				@dragleave="dragging = false"
				@drop.prevent="onDrop"
			>
				<label class="file">
					<span>Drop an IMF, DRO, VGM or OPX file here, or choose one</span>
					<input type="file" data-testid="file-input" accept=".imf,.wlf,.dro,.vgm,.opx,.bin" @change="onFileInput" />
				</label>
				<p v-if="fileName" class="note">{{ fileName }}</p>
			</div>
			<label class="rate">
				IMF rate (Hz)
				<input v-model.number="gridHz" type="number" min="1" max="65535" data-testid="grid-input" @change="reimport" />
			</label>
		</section>

		<section class="panel">
			<h2>Stream</h2>
			<dl v-if="stats" class="stats" data-testid="stats">
				<div v-for="row in rows" :key="row.id">
					<dt>{{ row.label }}</dt>
					<dd :data-testid="`stat-${row.id}`">{{ row.value }}</dd>
				</div>
			</dl>
			<p v-else class="note">No stream loaded.</p>
		</section>

		<section class="panel wide">
			<div class="toolbar">
				<div class="choices" role="radiogroup" aria-label="View">
					<label><input v-model="view" type="radio" name="view" value="header" data-testid="view-header" /> Binary header</label>
					<label><input v-model="view" type="radio" name="view" value="twin" data-testid="view-twin" /> Text twin</label>
				</div>
				<div class="actions">
					<button type="button" class="primary" data-testid="play" :disabled="!stats || !stats.valid || busy" @click="play">
						{{ busy ? 'Working…' : playing ? 'Play again' : 'Play' }}
					</button>
					<button type="button" data-testid="stop" :disabled="!playing" @click="stopPlayback">Stop</button>
				</div>
			</div>
			<pre v-if="view === 'header'" class="view" data-testid="header-view">{{ headerText }}</pre>
			<pre v-else class="view" data-testid="twin-view">{{ twinText }}</pre>
			<p class="status" role="status" data-testid="status">{{ status }}</p>
			<p v-if="peak !== null" class="note" data-testid="peak">Peak {{ peak.toFixed(4) }}</p>
			<p v-if="error" class="error" role="alert" data-testid="error">{{ error }}</p>
		</section>

		<section class="panel wide">
			<h2>One voice</h2>
			<p class="note">A single Undertone voice rendered by <code>undertone-dsp</code>, not the OPL emulator.</p>
			<div class="toolbar">
				<label>
					Waveform
					<select v-model="waveform" data-testid="voice-wave">
						<option>sine</option>
						<option>triangle</option>
						<option>square</option>
						<option>sawtooth</option>
					</select>
				</label>
				<label>
					MIDI note
					<input v-model.number="midi" type="number" min="0" max="127" data-testid="voice-midi" />
				</label>
				<button type="button" data-testid="play-voice" :disabled="!wasm" @click="playVoice">Play voice</button>
			</div>
		</section>
	</div>
</template>

<style scoped>
.demo {
	display: grid;
	grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
	gap: 16px;
	margin: 24px 0;
}

.panel {
	padding: 16px 20px;
	border: 1px solid var(--vp-c-divider);
	border-radius: 12px;
	background: var(--vp-c-bg-soft);
}

.panel.wide {
	grid-column: 1 / -1;
}

.panel h2 {
	margin: 0 0 12px;
	padding: 0;
	border: 0;
	font-size: 16px;
	letter-spacing: 0;
}

.choices {
	display: flex;
	flex-wrap: wrap;
	gap: 16px;
}

.note {
	margin: 4px 0 0;
	font-size: 13px;
	color: var(--vp-c-text-2);
}

.drop {
	margin: 12px 0;
	padding: 14px;
	border: 1px dashed var(--vp-c-divider);
	border-radius: 8px;
	font-size: 14px;
}

.drop.active,
.drop.over {
	border-color: var(--vp-c-brand-1);
}

.drop.over {
	background: var(--vp-c-brand-soft);
}

.file {
	display: flex;
	flex-direction: column;
	gap: 8px;
}

.rate {
	display: flex;
	align-items: center;
	gap: 8px;
	font-size: 14px;
}

input[type='number'],
select {
	width: 90px;
	padding: 4px 8px;
	border: 1px solid var(--vp-c-divider);
	border-radius: 6px;
	background: var(--vp-c-bg);
	color: var(--vp-c-text-1);
}

select {
	width: auto;
}

.stats {
	display: grid;
	grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
	gap: 12px;
	margin: 0;
}

.stats dt {
	font-size: 12px;
	color: var(--vp-c-text-2);
}

.stats dd {
	margin: 0;
	font-family: var(--vp-font-family-mono);
	font-size: 15px;
	color: var(--vp-c-brand-1);
}

.toolbar {
	display: flex;
	flex-wrap: wrap;
	align-items: center;
	justify-content: space-between;
	gap: 12px;
	font-size: 14px;
}

.actions {
	display: flex;
	gap: 8px;
}

button {
	padding: 6px 18px;
	border: 1px solid var(--vp-c-divider);
	border-radius: 999px;
	background: var(--vp-c-bg);
	color: var(--vp-c-text-1);
	font-weight: 500;
	cursor: pointer;
}

button.primary {
	border-color: var(--vp-c-brand-1);
	background: var(--vp-button-brand-bg);
	color: var(--vp-button-brand-text);
}

button:disabled {
	opacity: 0.5;
	cursor: not-allowed;
}

.view {
	max-height: 320px;
	margin: 12px 0 0;
	padding: 12px;
	overflow: auto;
	border-radius: 8px;
	background: var(--vp-c-bg);
	font-family: var(--vp-font-family-mono);
	font-size: 13px;
	line-height: 1.5;
}

.status {
	margin: 12px 0 0;
	font-size: 14px;
}

.error {
	margin: 8px 0 0;
	color: var(--vp-c-danger-1);
}
</style>
