<!--
	The browser demo: Title Theme three ways (the Undertone engine, an OPL2 chip file and an OPL3
	chip file), side by side, and any IMF, DRO, VGM or OPX file a visitor drops in.

	(c) Copyright 2026 Liminal HQ, Scott Morris
	SPDX-License-Identifier: Apache-2.0 OR MIT
-->
<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, shallowRef } from 'vue';
import wasmUrl from '../wasm/undertone_wasm_bg.wasm?url';
import graphUrl from '../../../../fixtures/undertone/graph.json?url';
import opl2Url from '../../../../fixtures/opx/title-theme.opl2.opx?url';
import opl3Url from '../../../../fixtures/opx/title-theme.opl3.opx?url';

type Wasm = typeof import('../wasm/undertone_wasm.js');
type Summary = import('../wasm/undertone_wasm.js').Summary;

type CardId = 'undertone' | 'opl2' | 'opl3' | 'file';

interface Row {
	id: string;
	label: string;
	value: string;
}

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

interface Measured {
	seconds: number;
	peak: number;
	peakLeft: number;
	peakRight: number;
}

/** The sample rate every version is rendered at, so the buffers do not depend on the sound card. */
const SAMPLE_RATE = 48_000;
/** `title-theme` in `fixtures/undertone/graph.json` is recorded at 140 bpm: 140 / 60 * 4 * 12. */
const TITLE_THEME_TICKS_PER_SECOND = 112;
/** Half a second for the chip's last notes to ring out. */
const CHIP_TAIL_SECONDS = 0.5;

const wasm = shallowRef<Wasm | null>(null);
const gridHz = ref(560);
const fileName = ref('');
const dragging = ref(false);
const inspect = ref<'opl2' | 'opl3' | 'file'>('opl2');
const view = ref<'header' | 'twin'>('header');
const error = ref('');
const status = ref('Loading the WebAssembly module…');
const busy = ref<CardId | null>(null);
const playingId = ref<CardId | null>(null);
const waveform = ref('triangle');
const midi = ref(69);

const stats = reactive<Record<'opl2' | 'opl3' | 'file', Stats | null>>({ opl2: null, opl3: null, file: null });
const undertoneStats = ref<{ events: number; voices: number; bars: number; passSeconds: number; frames: number } | null>(null);
const measured = reactive<Record<CardId, Measured | null>>({ undertone: null, opl2: null, opl3: null, file: null });
const cardError = reactive<Record<CardId, string>>({ undertone: '', opl2: '', opl3: '', file: '' });
const headers = reactive<Record<'opl2' | 'opl3' | 'file', string>>({ opl2: '', opl3: '', file: '' });
const twins = reactive<Record<'opl2' | 'opl3' | 'file', string>>({ opl2: '', opl3: '', file: '' });

let graphText = '';
const streams: Record<'opl2' | 'opl3' | 'file', Uint8Array | null> = { opl2: null, opl3: null, file: null };
let fileBytes: Uint8Array | null = null;
let context: AudioContext | null = null;
let playingSource: AudioBufferSourceNode | null = null;
let startedAt = 0;
let startOffset = 0;
const rendered = new Map<CardId, AudioBuffer>();
const rendering = new Map<CardId, Promise<AudioBuffer | null>>();

const number = new Intl.NumberFormat('en-CA');

const chipCards: { id: 'opl2' | 'opl3'; title: string; chip: string; blurb: string }[] = [
	{
		id: 'opl2',
		title: 'OPL2 chip',
		chip: 'OPL2',
		blurb:
			'The DOS arrangement of Title Theme lowered to the OPL2 by undertone-opl and written as OPX (title-theme.opl2.opx), then played through the oplon emulator. The OPL2 has one output, so both channels carry the same signal.'
	},
	{
		id: 'opl3',
		title: 'OPL3 chip',
		chip: 'OPL3',
		blurb:
			'The same DOS arrangement lowered to the OPL3 (title-theme.opl3.opx), played through oplon in OPL3 mode with the second register bank. Every voice is set to both outputs, four steps quieter, so the two channels carry the same notes.'
	}
];

function rowsFor(id: 'opl2' | 'opl3' | 'file'): Row[] {
	const s = stats[id];
	if (!s) return [];
	const m = measured[id];
	const rows: Row[] = [
		{ id: 'chip', label: 'Chip', value: s.chip },
		{ id: 'output', label: 'Output', value: s.chip === 'OPL3' ? 'stereo' : 'mono (both channels equal)' },
		{ id: 'grid', label: 'Grid', value: `${number.format(s.gridHz)} Hz` },
		{ id: 'ticks', label: 'Ticks', value: number.format(s.totalTicks) },
		{ id: 'seconds', label: 'Length', value: `${s.seconds.toFixed(2)} s` },
		{ id: 'writes', label: 'Writes', value: number.format(s.writes) },
		{ id: 'valid', label: 'Valid', value: s.valid ? 'yes' : 'no' },
		{ id: 'bytes', label: 'OPX size', value: `${number.format(s.fileLen)} bytes` },
		{ id: 'loop', label: 'Loop point', value: s.hasLoop ? 'yes' : 'none' }
	];
	if (m) rows.push(...measuredRows(m));
	return rows;
}

function measuredRows(m: Measured): Row[] {
	return [
		{ id: 'rendered', label: 'Rendered', value: `${m.seconds.toFixed(2)} s` },
		{ id: 'peak', label: 'Peak', value: m.peak.toFixed(4) },
		{ id: 'peak-lr', label: 'Peak L / R', value: `${m.peakLeft.toFixed(4)} / ${m.peakRight.toFixed(4)}` }
	];
}

const undertoneRows = computed<Row[]>(() => {
	const s = undertoneStats.value;
	if (!s) return [];
	const rows: Row[] = [
		{ id: 'engine', label: 'Engine', value: 'undertone-dsp' },
		{ id: 'output', label: 'Output', value: 'stereo, master bus' },
		{ id: 'events', label: 'Score events', value: number.format(s.events) },
		{ id: 'voices', label: 'Voices', value: number.format(s.voices) },
		{ id: 'bars', label: 'Bars', value: String(s.bars) },
		{ id: 'score', label: 'Score length', value: `${s.passSeconds.toFixed(2)} s` }
	];
	const m = measured.undertone;
	if (m) rows.push(...measuredRows(m));
	return rows;
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

/** Fills a chip card's stats, header table and twin from an OPX stream. */
function describeStream(id: 'opl2' | 'opl3' | 'file', opx: Uint8Array): void {
	const m = wasm.value;
	if (!m) return;
	streams[id] = opx;
	stats[id] = toStats(m.summarize(opx));
	headers[id] = m.headerTable(opx);
	twins[id] = stats[id]?.valid ? m.textTwin(opx) : '';
	if (!stats[id]?.valid) cardError[id] = `The stream is not valid: ${stats[id]?.error}`;
}

/** Imports the visitor's file and describes it. */
function loadFile(bytes: Uint8Array): void {
	const m = wasm.value;
	if (!m) return;
	forget('file');
	cardError.file = '';
	try {
		const format = m.detectFormat(bytes);
		const opx = m.importToOpx(bytes, format, gridHz.value);
		describeStream('file', opx);
		status.value = `Imported ${fileName.value} as ${format === 'opx' ? 'OPX (read as it is)' : format.toUpperCase()}.`;
		inspect.value = 'file';
	} catch (thrown) {
		streams.file = null;
		stats.file = null;
		headers.file = '';
		twins.file = '';
		cardError.file = describe(thrown);
		status.value = 'Nothing loaded from your file.';
	}
}

async function fetchBytes(url: string, what: string): Promise<Uint8Array> {
	const response = await fetch(url);
	if (!response.ok) throw new Error(`${what} could not be fetched (${response.status})`);
	return new Uint8Array(await response.arrayBuffer());
}

async function loadFixtures(): Promise<void> {
	const m = wasm.value;
	if (!m) return;
	try {
		const response = await fetch(graphUrl);
		if (!response.ok) throw new Error(`The recorded score could not be fetched (${response.status})`);
		graphText = await response.text();
		const probe = new m.ScorePlayer(graphText, 'title-theme', TITLE_THEME_TICKS_PER_SECOND, SAMPLE_RATE);
		undertoneStats.value = {
			events: probe.events(),
			voices: probe.voices(),
			bars: probe.bars(),
			passSeconds: probe.passSeconds(),
			frames: probe.totalFrames()
		};
		probe.free();
	} catch (thrown) {
		cardError.undertone = describe(thrown);
	}
	for (const [id, url] of [
		['opl2', opl2Url],
		['opl3', opl3Url]
	] as const) {
		try {
			describeStream(id, await fetchBytes(url, `The ${id.toUpperCase()} version`));
		} catch (thrown) {
			cardError[id] = describe(thrown);
		}
	}
}

async function useFile(file: File | undefined): Promise<void> {
	if (!file) return;
	fileName.value = file.name;
	fileBytes = new Uint8Array(await file.arrayBuffer());
	loadFile(fileBytes);
}

function onFileInput(event: Event): void {
	void useFile((event.target as HTMLInputElement).files?.[0]);
}

function onDrop(event: DragEvent): void {
	dragging.value = false;
	void useFile(event.dataTransfer?.files?.[0]);
}

function reimport(): void {
	if (fileBytes) loadFile(fileBytes);
}

/** Drops a card's rendered sound, because its stream changed. */
function forget(id: CardId): void {
	if (playingId.value === id) stopPlayback();
	rendered.delete(id);
	rendering.delete(id);
	measured[id] = null;
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
	playingId.value = null;
}

function toBuffer(interleaved: Float32Array): AudioBuffer {
	const frames = interleaved.length / 2;
	const buffer = new AudioBuffer({ numberOfChannels: 2, length: frames, sampleRate: SAMPLE_RATE });
	const left = buffer.getChannelData(0);
	const right = buffer.getChannelData(1);
	for (let i = 0; i < frames; i++) {
		left[i] = interleaved[2 * i];
		right[i] = interleaved[2 * i + 1];
	}
	return buffer;
}

function measure(buffer: AudioBuffer): Measured {
	const peakOf = (channel: number): number => {
		let max = 0;
		for (const sample of buffer.getChannelData(channel)) max = Math.max(max, Math.abs(sample));
		return max;
	};
	const peakLeft = peakOf(0);
	const peakRight = peakOf(1);
	return { seconds: buffer.duration, peak: Math.max(peakLeft, peakRight), peakLeft, peakRight };
}

interface Renderer {
	render(frames: number): Float32Array;
	free(): void;
}

/** Renders `total` frames in blocks, letting the page paint between them. */
async function renderStream(id: CardId, renderer: Renderer, total: number): Promise<AudioBuffer> {
	const block = 16_384;
	const interleaved = new Float32Array(total * 2);
	let done = 0;
	try {
		while (done < total) {
			const frames = Math.min(block, total - done);
			interleaved.set(renderer.render(frames), done * 2);
			done += frames;
			status.value = `Rendering ${label(id)}… ${Math.round((100 * done) / total)}%`;
			await new Promise((resolve) => setTimeout(resolve, 0));
		}
	} finally {
		renderer.free();
	}
	return toBuffer(interleaved);
}

function label(id: CardId): string {
	return { undertone: 'the Undertone version', opl2: 'the OPL2 version', opl3: 'the OPL3 version', file: 'your file' }[id];
}

/** The rendered sound of a card, made once and kept. */
function ensureRendered(id: CardId): Promise<AudioBuffer | null> {
	const cached = rendered.get(id);
	if (cached) return Promise.resolve(cached);
	const pending = rendering.get(id);
	if (pending) return pending;
	const m = wasm.value;
	if (!m) return Promise.resolve(null);
	const job = (async (): Promise<AudioBuffer | null> => {
		try {
			let buffer: AudioBuffer;
			if (id === 'undertone') {
				const player = new m.ScorePlayer(graphText, 'title-theme', TITLE_THEME_TICKS_PER_SECOND, SAMPLE_RATE);
				buffer = await renderStream(id, player, player.totalFrames());
			} else {
				const opx = streams[id];
				if (!opx) return null;
				const player = new m.Player(opx, SAMPLE_RATE);
				const total = player.totalFrames() + Math.round(SAMPLE_RATE * CHIP_TAIL_SECONDS);
				buffer = await renderStream(id, player, total);
			}
			if (rendering.get(id) !== job) return null;
			rendered.set(id, buffer);
			measured[id] = measure(buffer);
			status.value = `Rendered ${label(id)}: ${buffer.duration.toFixed(2)} s, peak ${measured[id]?.peak.toFixed(4)}.`;
			return buffer;
		} catch (thrown) {
			cardError[id] = describe(thrown);
			status.value = 'Rendering failed.';
			return null;
		} finally {
			if (rendering.get(id) === job) rendering.delete(id);
		}
	})();
	rendering.set(id, job);
	return job;
}

/** Where the playing version is, in seconds. */
function position(): number {
	return context ? startOffset + (context.currentTime - startedAt) : 0;
}

const chipIds: CardId[] = ['opl2', 'opl3'];

async function play(id: CardId): Promise<void> {
	if (busy.value) return;
	error.value = '';
	const ctx = audio();
	await ctx.resume();
	// The two chip versions are one arrangement on one grid, so switching between them while one
	// plays carries on from the same moment. The Undertone version is another arrangement and
	// always starts from its beginning.
	const carryOn = playingId.value !== null && playingId.value !== id && chipIds.includes(id) && chipIds.includes(playingId.value);
	const from = carryOn ? position() : 0;
	busy.value = id;
	const buffer = await ensureRendered(id);
	busy.value = null;
	if (!buffer) return;
	stopPlayback();
	const node = ctx.createBufferSource();
	node.buffer = buffer;
	node.connect(ctx.destination);
	node.onended = () => {
		if (playingSource === node) stopPlayback();
	};
	playingSource = node;
	playingId.value = id;
	startOffset = from < buffer.duration ? from : 0;
	startedAt = ctx.currentTime;
	node.start(0, startOffset);
	status.value = carryOn
		? `Playing ${label(id)} from ${startOffset.toFixed(1)} s, where the other chip version was.`
		: `Playing ${label(id)} from the start.`;
}

function playVoice(): void {
	const m = wasm.value;
	if (!m) return;
	error.value = '';
	try {
		const ctx = audio();
		void ctx.resume();
		const samples = m.renderVoice(waveform.value, midi.value, 1, SAMPLE_RATE);
		const buffer = toBuffer(samples);
		status.value = `Rendered one ${waveform.value} voice through undertone-dsp. Peak ${measure(buffer).peak.toFixed(4)}.`;
		stopPlayback();
		const node = ctx.createBufferSource();
		node.buffer = buffer;
		node.connect(ctx.destination);
		node.onended = () => {
			if (playingSource === node) stopPlayback();
		};
		playingSource = node;
		node.start();
	} catch (thrown) {
		error.value = describe(thrown);
	}
}

const inspectable = computed(() => (['opl2', 'opl3', 'file'] as const).filter((id) => id !== 'file' || stats.file !== null));

onMounted(async () => {
	try {
		const m = await import('../wasm/undertone_wasm.js');
		await m.default({ module_or_path: wasmUrl });
		wasm.value = m;
		status.value = 'Ready.';
		await loadFixtures();
		// Render each version in turn, so the stats show what each one sounds like at once.
		for (const id of ['undertone', 'opl2', 'opl3'] as const) await ensureRendered(id);
		status.value = 'Ready. Press Play on a version.';
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
		<p class="intro note wide">
			One song, three engines. Press Play on a version; only one plays at a time. Switching between the two chip versions while one plays carries on from the same moment, because they are one arrangement on one grid. The Undertone version is a different arrangement with a different length, so it always starts from its beginning.
		</p>

		<section class="card" data-testid="card-undertone">
			<h2>Undertone engine</h2>
			<p class="note">
				The original, software-synthesised version: the recorded <code>title-theme</code> score from <code>fixtures/undertone/graph.json</code>, the one the Rust tests check, booked and rendered in your browser by <code>undertone-dsp</code> as rich software voices through the master bus. It is the first two bars of the studio's eight-bar arrangement, not the whole song, and not the DOS arrangement the chip versions play.
			</p>
			<dl v-if="undertoneRows.length" class="stats" data-testid="stats-undertone">
				<div v-for="row in undertoneRows" :key="row.id">
					<dt>{{ row.label }}</dt>
					<dd :data-testid="`undertone-${row.id}`">{{ row.value }}</dd>
				</div>
			</dl>
			<p v-else class="note">Not loaded yet.</p>
			<div class="actions">
				<button type="button" class="primary" data-testid="play-undertone" :disabled="!undertoneStats || busy !== null" @click="play('undertone')">
					{{ busy === 'undertone' ? 'Rendering…' : playingId === 'undertone' ? 'Play again' : 'Play' }}
				</button>
				<button type="button" data-testid="stop-undertone" :disabled="playingId !== 'undertone'" @click="stopPlayback">Stop</button>
			</div>
			<p v-if="cardError.undertone" class="error" role="alert" data-testid="error-undertone">{{ cardError.undertone }}</p>
		</section>

		<section v-for="card in chipCards" :key="card.id" class="card" :data-testid="`card-${card.id}`">
			<h2>{{ card.title }}</h2>
			<p class="note">{{ card.blurb }}</p>
			<dl v-if="stats[card.id]" class="stats" :data-testid="`stats-${card.id}`">
				<div v-for="row in rowsFor(card.id)" :key="row.id">
					<dt>{{ row.label }}</dt>
					<dd :data-testid="`${card.id}-${row.id}`">{{ row.value }}</dd>
				</div>
			</dl>
			<p v-else class="note">Not loaded yet.</p>
			<div class="actions">
				<button type="button" class="primary" :data-testid="`play-${card.id}`" :disabled="!stats[card.id]?.valid || busy !== null" @click="play(card.id)">
					{{ busy === card.id ? 'Rendering…' : playingId === card.id ? 'Play again' : 'Play' }}
				</button>
				<button type="button" :data-testid="`stop-${card.id}`" :disabled="playingId !== card.id" @click="stopPlayback">Stop</button>
			</div>
			<p v-if="cardError[card.id]" class="error" role="alert" :data-testid="`error-${card.id}`">{{ cardError[card.id] }}</p>
		</section>

		<p class="note wide" data-testid="honesty">
			The Undertone version and the chip versions are different arrangements of Title Theme: rich software voices on one side, DOS chip music on the other. The chip files are lowered from the studio's DOS arrangement (<code>title-theme-dos</code>, 36 bars at 140 bpm), so the notes are not the same ones. The figures on each card are measured from the samples rendered in your browser.
		</p>

		<section class="panel wide">
			<h2>Your own file</h2>
			<div
				class="drop"
				:class="{ over: dragging, active: stats.file !== null }"
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
			<dl v-if="stats.file" class="stats" data-testid="stats-file">
				<div v-for="row in rowsFor('file')" :key="row.id">
					<dt>{{ row.label }}</dt>
					<dd :data-testid="`file-${row.id}`">{{ row.value }}</dd>
				</div>
			</dl>
			<div class="actions">
				<button type="button" class="primary" data-testid="play-file" :disabled="!stats.file?.valid || busy !== null" @click="play('file')">
					{{ busy === 'file' ? 'Rendering…' : playingId === 'file' ? 'Play again' : 'Play' }}
				</button>
				<button type="button" data-testid="stop-file" :disabled="playingId !== 'file'" @click="stopPlayback">Stop</button>
			</div>
			<p v-if="cardError.file" class="error" role="alert" data-testid="error-file">{{ cardError.file }}</p>
		</section>

		<section class="panel wide">
			<div class="toolbar">
				<div class="choices" role="radiogroup" aria-label="Stream">
					<label v-for="id in inspectable" :key="id">
						<input v-model="inspect" type="radio" name="inspect" :value="id" :data-testid="`inspect-${id}`" />
						{{ id === 'file' ? 'Your file' : id.toUpperCase() + ' version' }}
					</label>
				</div>
				<div class="choices" role="radiogroup" aria-label="View">
					<label><input v-model="view" type="radio" name="view" value="header" data-testid="view-header" /> Binary header</label>
					<label><input v-model="view" type="radio" name="view" value="twin" data-testid="view-twin" /> Text twin</label>
				</div>
			</div>
			<pre v-if="view === 'header'" class="view" data-testid="header-view">{{ headers[inspect] }}</pre>
			<pre v-else class="view" data-testid="twin-view">{{ twins[inspect] }}</pre>
			<p class="status" role="status" data-testid="status">{{ status }}</p>
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
	grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
	gap: 16px;
	margin: 24px 0;
}

.card,
.panel {
	padding: 16px 20px;
	border: 1px solid var(--vp-c-divider);
	border-radius: 12px;
	background: var(--vp-c-bg-soft);
}

.wide {
	grid-column: 1 / -1;
}

.card h2,
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

.card .actions {
	margin-top: 16px;
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
