<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { get } from 'svelte/store';
  import { tasks } from '../stores/tasks';
  import { machines } from '../stores/machines';
  import { refreshMachinesStore } from '../stores/machines';
  import { session, atlasAgentInfo, pixelAgentInfo, atlasFeed, pixelFeed } from '$lib/stores/session';
  import { getAgentMessages } from '../api';
  import { listen } from '@tauri-apps/api/event';
  import { appVisible } from '../stores/visibility';
  import { t, locale } from '$lib/i18n';

  import type { RoomDef, AgentState, Bubble, Particle, Mote, ChatLine, ActivityItem } from './pixelrooms/types';
  import type { BgStar } from './pixelrooms/renderer';
  import type { SessionData, AgentInfo } from '$lib/types';

  // --- Planning beam types ---
  interface BeamParticle {
    t: number; speed: number; size: number; color: string; alpha: number; offset: number;
  }
  interface PlanningText {
    sender: string; content: string; round: number; alpha: number; y: number;
  }
  import {
    getRoomRect, getOrbCenter, getOrbRadius,
    renderBackground, drawRoomOrb,
    drawSubAgents, getSubAgentMotePositions,
    drawMotes, drawParticles, drawBubbles,
  } from './pixelrooms/renderer';

  // --- Constants ---
  const ROOMS: RoomDef[] = [
    { id: 'atlas', name: 'ATLAS', primary: '#00BCD4', glow: 'rgba(0,188,212,0.12)', core: '#80DEEA' },
    { id: 'pixel', name: 'PIXEL', primary: '#4CAF50', glow: 'rgba(76,175,80,0.12)', core: '#A5D6A7' },
    { id: 'nova',  name: 'NOVA',  primary: '#FF9800', glow: 'rgba(255,152,0,0.12)', core: '#FFCC80' },
    { id: 'nomad', name: 'NOMAD', primary: '#9C27B0', glow: 'rgba(156,39,176,0.12)', core: '#CE93D8' },
  ];
  const BUBBLE_DURATION = 5;
  const CHAT_MAX = 5;

  // --- Visibility ---
  let visible = true;

  // --- State ---
  let canvas: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D;
  let canvasW = $state(800);
  let canvasH = $state(400);
  let animFrame = 0;
  let frameCount = 0;
  let lastTime = 0;
  let lastFrameTime = 0;
  const FRAME_INTERVAL = 1000 / 30;
  let agents: Record<string, AgentState> = {};
  let bubbles: Bubble[] = [];
  let particles: Particle[] = [];
  let motes: Mote[] = [];
  let chatLog: ChatLine[] = [];

  // DOM-reactive state
  let tooltip = $state<{ x: number; y: number; lines: string[] } | null>(null);
  let popup = $state<{ machineId: string; x: number; y: number } | null>(null);
  let hoveredRoom: string | null = null;
  let hoveredAgent: string | null = null;

  // Cached activity feeds
  let cachedAtlasFeed: ActivityItem[] = [];
  let cachedPixelFeed: ActivityItem[] = [];

  // Cached store values for draw loop (populated in syncAgentsFromTasks, ~3s interval)
  let cachedSession: SessionData | null = null;
  let cachedAtlasAgentInfo: AgentInfo | null = null;
  let cachedPixelAgentInfo: AgentInfo | null = null;

  // Planning visualization state
  let planningActive = false;
  let planningSpeaker = '';
  let planningRound = 0;
  let beamParticles: BeamParticle[] = [];
  let beamIntensity = 0;
  let planningTexts: PlanningText[] = [];

  // Offscreen canvas for static background
  let bgCanvas: HTMLCanvasElement | null = null;
  let bgW = 0;
  let bgH = 0;

  // Background stars
  let bgStars: BgStar[] = [];

  // Track seen message IDs to avoid re-adding duplicates
  let seenMessageIds = new Set<string>();

  // Cleanup function populated by async onMount, called by onDestroy
  let cleanup: (() => void) | null = null;
  onDestroy(() => cleanup?.());

  // --- Canvas Setup ---
  onMount(async () => {
    const unsubVisible = appVisible.subscribe(v => visible = v);

    const rawCtx = canvas.getContext('2d');
    if (!rawCtx) return;
    ctx = rawCtx;
    ctx.imageSmoothingEnabled = true;

    refreshMachinesStore();

    for (let i = 0; i < 80; i++) {
      bgStars.push({
        x: Math.random(), y: Math.random(),
        r: 0.3 + Math.random() * 1.2,
        a: 0.2 + Math.random() * 0.6,
        speed: 0.2 + Math.random() * 0.8,
      });
    }

    const ro = new ResizeObserver((entries) => {
      const rect = entries[0].contentRect;
      canvasW = Math.floor(rect.width);
      canvasH = Math.floor(rect.height);
      canvas.width = canvasW;
      canvas.height = canvasH;
    });
    ro.observe(canvas.parentElement!);

    const parent = canvas.parentElement!;
    canvasW = parent.clientWidth;
    canvasH = parent.clientHeight;
    canvas.width = canvasW;
    canvas.height = canvasH;

    lastTime = performance.now();
    gameLoop(lastTime);

    const machineInterval = setInterval(refreshMachinesStore, 60000);

    // Tauri event listeners
    const unlistenDone = await listen<{ id: number; target: string; output: string }>('task-done', (e) => {
      const isError = e.payload.output?.toLowerCase().includes('error') || e.payload.output?.toLowerCase().includes('fail');
      if (isError) {
        addBubble(e.payload.target, t('pixel.error', { msg: (e.payload.output || '').slice(0, 30) }), '#ff3355');
        setAgentState(e.payload.target, 'error', 3);
      } else {
        addBubble(e.payload.target, t('pixel.done'), '#00ff88');
        setAgentState(e.payload.target, 'completing', 2);
      }
      addChat(e.payload.target, isError ? t('common.error') : t('pixel.taskComplete'));
    });

    const unlistenConflict = await listen<{ message: string }>('repo-conflict', (e) => {
      for (const r of ROOMS) addBubble(r.id, t('pixel.conflict'), '#ffb800');
      addChat('JARVIS', e.payload.message);
    });

    const unlistenRule = await listen<{ rule: string; message: string }>('rule-alert', (e) => {
      addBubble('atlas', t('pixel.rule', { name: e.payload.rule.slice(0, 20) }), '#9C27B0');
      addChat('Regla', e.payload.message.slice(0, 40));
    });

    const unlistenStarted = await listen<{ id: number; target: string }>('task-started', (e) => {
      setAgentState(e.payload.target, 'working', 0);
    });

    // Planning mode
    let lastPlanningMsgCount = 0;
    let lastPlanningSpeaker = '';
    const unlistenPlanning = await listen<any>('planning-update', (e) => {
      const state = e.payload;
      if (!state) return;

      const msgs = state.messages || [];
      const speaker = state.current_speaker || '';
      const phase = state.phase || '';

      // Update planning visual state
      planningActive = phase === 'planning' || phase === 'executing';
      planningSpeaker = speaker;
      planningRound = state.current_round || 0;

      if (speaker && phase !== 'done' && phase !== 'idle') {
        setAgentState(speaker, 'working', 0);
        const other = speaker === 'atlas' ? 'pixel' : 'atlas';
        if (agents[other]?.state === 'working' && agents[other]?.stateTimer === 0) {
          agents[other].state = 'idle';
        }
      }

      if (msgs.length > lastPlanningMsgCount) {
        const newMsgs = msgs.slice(lastPlanningMsgCount);
        for (const m of newMsgs) {
          const from = m.speaker || m.role || 'agent';
          const text = (m.content || m.text || '').slice(0, 35);
          const fullText = (m.content || m.text || '').slice(0, 120);
          if (text) {
            const color = from === 'atlas' ? '#00BCD4' : from === 'pixel' ? '#4CAF50' : '#60a5fa';
            addBubble(from, text, color);
            addChat(from.toUpperCase(), text.slice(0, 40));

            // Add planning text panel
            planningTexts.push({
              sender: from, content: fullText,
              round: m.round || planningRound,
              alpha: 1.0, y: 0,
            });
            if (planningTexts.length > 6) planningTexts.shift();

            // Spawn beam burst for message exchange
            const fromColor = from === 'atlas' ? '#00BCD4' : '#4CAF50';
            for (let i = 0; i < 10; i++) {
              beamParticles.push({
                t: from === 'atlas' ? 0 : 1,
                speed: (0.003 + Math.random() * 0.005) * (from === 'atlas' ? 1 : -1),
                size: 1.5 + Math.random() * 2.5,
                color: fromColor,
                alpha: 0.6 + Math.random() * 0.4,
                offset: (Math.random() - 0.5) * 25,
              });
            }
            beamIntensity = 1.0;
          }
        }
        lastPlanningMsgCount = msgs.length;
      }

      if (speaker !== lastPlanningSpeaker && speaker) {
        addBubble(speaker, t('pixel.roundThinking', { n: state.current_round || '?' }), speaker === 'atlas' ? '#00BCD4' : '#4CAF50');
        lastPlanningSpeaker = speaker;
        // Burst particles from new speaker
        const roomIdx = ROOMS.findIndex(r => r.id === speaker);
        if (roomIdx >= 0) {
          const center = getOrbCenter(roomIdx, ROOMS.length, canvasW, canvasH);
          spawnParticles(center.x, center.y, [speaker === 'atlas' ? '#00BCD4' : '#4CAF50', '#ffffff'], 15, 4);
        }
        beamIntensity = 0.8;
      }

      if (phase === 'done' || phase === 'finished') {
        for (const r of ROOMS) setAgentState(r.id, 'completing', 3);
        addBubble('atlas', t('pixel.planComplete'), '#00ff88');
        addChat('JARVIS', t('pixel.planFinished'));
        lastPlanningMsgCount = 0;
        lastPlanningSpeaker = '';
        planningActive = false;
        planningTexts = [];
      }
    });

    const msgInterval = setInterval(async () => {
      try {
        const msgs = await getAgentMessages(undefined, true);
        for (const m of msgs.slice(-2)) {
          // Deduplicate: only show each message once
          const msgKey = `${m.from}:${m.to}:${m.content.slice(0, 30)}:${m.timestamp || ''}`;
          if (seenMessageIds.has(msgKey)) continue;
          seenMessageIds.add(msgKey);
          // Cap set size to prevent unbounded growth
          if (seenMessageIds.size > 50) {
            const first = Array.from(seenMessageIds)[0];
            if (first !== undefined) seenMessageIds.delete(first);
          }
          addBubble(m.to === 'all' ? 'atlas' : m.to, `${m.from}: ${m.content.slice(0, 25)}`, '#60a5fa');
          addChat(m.from, m.content.slice(0, 40));
        }
      } catch { /* ignore */ }
    }, 15000);

    cleanup = () => {
      cancelAnimationFrame(animFrame);
      clearInterval(machineInterval);
      clearInterval(msgInterval);
      unsubVisible();
      ro.disconnect();
      unlistenDone();
      unlistenConflict();
      unlistenRule();
      unlistenStarted();
      unlistenPlanning();
    };
  });

  // --- Sync agents from tasks & session ---
  function syncAgentsFromTasks() {
    let currentTasks: typeof $tasks;
    try { currentTasks = $tasks; } catch { return; }
    const now = Date.now();
    const STALE_MS = 30 * 60 * 1000;
    const running = currentTasks.filter(t =>
      t.status === 'running' && t.startedAt && (now - t.startedAt) < STALE_MS
    );
    for (const r of ROOMS) {
      const task = running.find(t => t.target === r.id);
      const prev = agents[r.id];
      if (task) {
        if (!prev || (prev.state !== 'error' && prev.state !== 'completing')) {
          agents[r.id] = {
            machineId: r.id, state: 'working',
            taskPrompt: task.prompt,
            elapsed: Math.floor((now - (task.startedAt || now)) / 1000),
            stateTimer: prev?.stateTimer || 0,
          };
        } else if (prev) {
          prev.taskPrompt = task.prompt;
          prev.elapsed = Math.floor((now - (task.startedAt || now)) / 1000);
        }
      } else {
        if (!prev) {
          agents[r.id] = { machineId: r.id, state: 'idle', taskPrompt: '', elapsed: 0, stateTimer: 0 };
        } else if (prev.state !== 'error' && prev.state !== 'completing') {
          prev.state = 'idle';
          prev.taskPrompt = '';
          prev.elapsed = 0;
        }
      }
    }

    try {
      const sess = get(session);
      const atlasInfo = get(atlasAgentInfo);
      const pixelInfo = get(pixelAgentInfo);
      // Cache for draw loop use (avoids per-frame store reads)
      cachedSession = sess;
      cachedAtlasAgentInfo = atlasInfo;
      cachedPixelAgentInfo = pixelInfo;
      if ((sess.atlasRunning || atlasInfo.agentCount > 0) && agents['atlas']?.state === 'idle') {
        agents['atlas'] = { ...agents['atlas'], state: 'working', taskPrompt: t('pixel.instances', { count: atlasInfo.agentCount }), elapsed: 0 };
      }
      if ((sess.pixelRunning || pixelInfo.agentCount > 0) && agents['pixel']?.state === 'idle') {
        agents['pixel'] = { ...agents['pixel'], state: 'working', taskPrompt: t('pixel.instances', { count: pixelInfo.agentCount }), elapsed: 0 };
      }
    } catch { /* stores not ready */ }

    try { cachedAtlasFeed = get(atlasFeed); } catch { /* */ }
    try { cachedPixelFeed = get(pixelFeed); } catch { /* */ }
  }

  // --- Helpers ---
  function setAgentState(machineId: string, state: AgentState['state'], duration: number) {
    const a = agents[machineId];
    if (a) { a.state = state; a.stateTimer = duration; }
  }

  function addBubble(machineId: string, text: string, color: string) {
    // Skip duplicates of currently visible bubbles
    const truncated = text.slice(0, 35) + (text.length > 35 ? '...' : '');
    if (bubbles.some(b => b.machineId === machineId && b.text === truncated && b.timer > 0)) return;
    while (bubbles.length > 4) bubbles.shift();
    bubbles.push({ machineId, text: truncated, color, timer: BUBBLE_DURATION, maxTimer: BUBBLE_DURATION });
  }

  function addChat(agent: string, text: string) {
    const time = new Date().toLocaleTimeString(get(locale), { hour: '2-digit', minute: '2-digit' });
    chatLog.push({ time, agent, text, age: 0 });
    if (chatLog.length > CHAT_MAX) chatLog.shift();
  }

  function spawnParticles(x: number, y: number, colors: string[], count: number, spread: number) {
    for (let i = 0; i < count; i++) {
      particles.push({
        x, y,
        vx: (Math.random() - 0.5) * spread,
        vy: -Math.random() * spread * 1.2,
        color: colors[Math.floor(Math.random() * colors.length)],
        life: 1, maxLife: 0.8 + Math.random() * 0.7,
        size: 1 + Math.random() * 2,
      });
    }
    if (particles.length > 80) particles.splice(0, particles.length - 80);
  }

  function spawnMotes(x: number, y: number, color: string, count: number) {
    for (let i = 0; i < count; i++) {
      const angle = Math.random() * Math.PI * 2;
      const speed = 0.3 + Math.random() * 0.8;
      motes.push({
        x, y,
        vx: Math.cos(angle) * speed, vy: Math.sin(angle) * speed,
        r: 1 + Math.random() * 2.5, color,
        alpha: 0.4 + Math.random() * 0.4,
        decay: 0.003 + Math.random() * 0.005,
      });
    }
  }

  // --- Background ---
  function ensureBgCanvas() {
    if (bgCanvas && bgW === canvasW && bgH === canvasH) return;
    bgCanvas = document.createElement('canvas');
    bgCanvas.width = canvasW;
    bgCanvas.height = canvasH;
    bgW = canvasW;
    bgH = canvasH;
    const bgCtx = bgCanvas.getContext('2d');
    if (!bgCtx) return;
    renderBackground(bgCtx, ROOMS, bgStars, canvasW, canvasH);
  }

  // --- Room drawing (orchestrates renderer calls) ---
  function drawRoom(room: RoomDef, index: number) {
    const machine = Object.values($machines).find(m => m.id === room.id);
    const isOffline = !!(machine && (!machine.enabled || machine.health?.online === false));
    const agState = agents[room.id]?.state || 'idle';
    const agentInfo = room.id === 'atlas' ? cachedAtlasAgentInfo :
                      room.id === 'pixel' ? cachedPixelAgentInfo : null;

    const vis = drawRoomOrb({
      ctx, room, index, isOffline, agState,
      agentCount: agentInfo?.agentCount || 0,
      roomCount: ROOMS.length, canvasW, canvasH, frameCount,
    });

    // Handle side-effects (mote/particle spawning)
    const center = getOrbCenter(index, ROOMS.length, canvasW, canvasH);
    if (vis.shouldSpawnMotes) spawnMotes(center.x, center.y, vis.moteColor, 1);
    if (vis.shouldSpawnParticles) spawnParticles(center.x, center.y, vis.particleColors, 2, 3);
  }

  // --- Sub-agents orchestration ---
  function drawRoomSubAgents(room: RoomDef, index: number) {
    const info = room.id === 'atlas' ? cachedAtlasAgentInfo :
                 room.id === 'pixel' ? cachedPixelAgentInfo : null;
    const count = info ? Math.min(info.agentCount, 8) - 1 : 0;
    if (count <= 0) return;

    const center = getOrbCenter(index, ROOMS.length, canvasW, canvasH);
    const mainR = getOrbRadius(index, ROOMS.length, canvasW, canvasH);
    const isWorking = agents[room.id]?.state === 'working';

    drawSubAgents({ ctx, room, center, mainR, count, frameCount, isWorking });

    // Spawn motes from sub-agents
    const motePositions = getSubAgentMotePositions(room, center, mainR, count, frameCount, isWorking);
    for (const pos of motePositions) spawnMotes(pos.x, pos.y, pos.color, 1);
  }

  // --- Data transmission & planning beam rendering ---
  function drawPlanningBeam(dt: number) {
    const atlasIdx = ROOMS.findIndex(r => r.id === 'atlas');
    const pixelIdx = ROOMS.findIndex(r => r.id === 'pixel');
    if (atlasIdx < 0 || pixelIdx < 0) return;

    const ac = getOrbCenter(atlasIdx, ROOMS.length, canvasW, canvasH);
    const pc = getOrbCenter(pixelIdx, ROOMS.length, canvasW, canvasH);
    const r = getOrbRadius(0, ROOMS.length, canvasW, canvasH);
    const t = frameCount * 0.015;

    // Check if any agent is active for intensity (use cached value from syncAgentsFromTasks)
    let anyActive = false;
    try {
      if (cachedSession) anyActive = cachedSession.atlasRunning || cachedSession.pixelRunning;
    } catch { /* */ }

    const activeLevel = planningActive ? 1.0 : anyActive ? 0.5 : 0.15;
    const baseAlpha = 0.03 + (activeLevel * 0.12) + beamIntensity * 0.15;

    // Wavy energy beams (always on, intensity varies)
    ctx.save();
    ctx.globalAlpha = baseAlpha;
    const beamCount = planningActive ? 4 : anyActive ? 3 : 2;
    for (let w = 0; w < beamCount; w++) {
      ctx.beginPath();
      ctx.moveTo(ac.x + r, ac.y);
      const steps = 40;
      for (let i = 1; i <= steps; i++) {
        const frac = i / steps;
        const x = ac.x + r + (pc.x - r - ac.x - r) * frac;
        const waveAmp = 4 + activeLevel * 14 + beamIntensity * 10;
        const waveY = Math.sin(frac * Math.PI * 3 + t * 2.5 + w * 0.8) * waveAmp;
        const y = (ac.y + pc.y) / 2 + waveY;
        ctx.lineTo(x, y);
      }
      const grad = ctx.createLinearGradient(ac.x, 0, pc.x, 0);
      grad.addColorStop(0, '#00BCD4');
      grad.addColorStop(0.5, planningActive ? '#ffffff' : '#60a5fa');
      grad.addColorStop(1, '#4CAF50');
      ctx.strokeStyle = grad;
      ctx.lineWidth = 0.5 + activeLevel * 1.5 + beamIntensity * 2;
      ctx.stroke();
    }
    ctx.restore();

    // Always spawn ambient data particles (rate varies by activity)
    const spawnRate = planningActive ? 4 : anyActive ? 8 : 16;
    if (frameCount % spawnRate === 0) {
      const fromAtlas = Math.random() > 0.5;
      beamParticles.push({
        t: fromAtlas ? 0 : 1,
        speed: (0.002 + Math.random() * 0.004) * (fromAtlas ? 1 : -1),
        size: 0.8 + Math.random() * (planningActive ? 2.5 : 1.5),
        color: fromAtlas ? '#00BCD4' : '#4CAF50',
        alpha: 0.2 + Math.random() * (planningActive ? 0.5 : 0.3),
        offset: (Math.random() - 0.5) * (planningActive ? 25 : 15),
      });
    }

    // Update beam particles
    const startX = ac.x + r;
    const endX = pc.x - r;
    const midY = (ac.y + pc.y) / 2;
    for (let i = beamParticles.length - 1; i >= 0; i--) {
      const bp = beamParticles[i];
      bp.t += bp.speed;
      bp.alpha -= 0.003;
      if (bp.t < -0.1 || bp.t > 1.1 || bp.alpha <= 0) {
        beamParticles.splice(i, 1);
        continue;
      }
      const x = startX + (endX - startX) * bp.t;
      const waveY = Math.sin(bp.t * Math.PI * 2 + t * 2) * 6;
      const y = midY + waveY + bp.offset;

      ctx.globalAlpha = bp.alpha;
      const grd = ctx.createRadialGradient(x, y, 0, x, y, bp.size * 2.5);
      grd.addColorStop(0, bp.color);
      grd.addColorStop(1, 'transparent');
      ctx.fillStyle = grd;
      ctx.beginPath();
      ctx.arc(x, y, bp.size * 2.5, 0, Math.PI * 2);
      ctx.fill();

      ctx.fillStyle = bp.color;
      ctx.beginPath();
      ctx.arc(x, y, bp.size, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
    if (beamParticles.length > 100) beamParticles.splice(0, beamParticles.length - 100);

    // Decay beam intensity
    beamIntensity = Math.max(0, beamIntensity - dt * 0.25);
  }

  function drawPlanningTexts(dt: number) {
    if (planningTexts.length === 0) return;

    const atlasIdx = ROOMS.findIndex(r => r.id === 'atlas');
    const pixelIdx = ROOMS.findIndex(r => r.id === 'pixel');
    if (atlasIdx < 0 || pixelIdx < 0) return;

    const ac = getOrbCenter(atlasIdx, ROOMS.length, canvasW, canvasH);
    const pc = getOrbCenter(pixelIdx, ROOMS.length, canvasW, canvasH);
    const r = getOrbRadius(0, ROOMS.length, canvasW, canvasH);

    // Show last 3 messages as holographic panels near their orb
    const visible = planningTexts.slice(-3);
    for (let i = 0; i < visible.length; i++) {
      const pt = visible[i];
      pt.alpha = Math.max(0, pt.alpha - dt * 0.03);
      if (pt.alpha <= 0) continue;

      const isAtlas = pt.sender === 'atlas';
      const center = isAtlas ? ac : pc;
      const panelX = isAtlas ? center.x - r * 2.8 : center.x + r * 1.2;
      const panelY = center.y - r * 0.5 + i * 55;
      const panelW = Math.min(r * 3, canvasW * 0.22);
      const panelH = 48;

      ctx.globalAlpha = pt.alpha * 0.75;

      // Panel background
      ctx.fillStyle = 'rgba(2, 8, 16, 0.7)';
      ctx.strokeStyle = isAtlas ? '#00BCD430' : '#4CAF5030';
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.roundRect(panelX, panelY, panelW, panelH, 6);
      ctx.fill();
      ctx.stroke();

      // Sender label
      ctx.font = '700 8px "Chakra Petch", sans-serif';
      ctx.textAlign = 'left';
      ctx.fillStyle = isAtlas ? '#00BCD4' : '#4CAF50';
      ctx.fillText(`${pt.sender.toUpperCase()} R${pt.round}`, panelX + 8, panelY + 13);

      // Content (truncated, wrapped)
      ctx.font = '400 9px "IBM Plex Mono", monospace';
      ctx.fillStyle = '#8899aa';
      const maxChars = Math.floor((panelW - 16) / 5.5);
      const line1 = pt.content.slice(0, maxChars);
      const line2 = pt.content.slice(maxChars, maxChars * 2);
      ctx.fillText(line1, panelX + 8, panelY + 27, panelW - 16);
      if (line2) ctx.fillText(line2, panelX + 8, panelY + 40, panelW - 16);
    }
    ctx.globalAlpha = 1;

    // Round indicator in the center
    if (planningActive && planningRound > 0) {
      const midX = (ac.x + pc.x) / 2;
      const midY = Math.min(ac.y, pc.y) - r * 1.5;

      ctx.font = '700 10px "Chakra Petch", sans-serif';
      ctx.textAlign = 'center';
      ctx.fillStyle = 'rgba(255,255,255,0.08)';
      ctx.beginPath();
      ctx.roundRect(midX - 45, midY - 12, 90, 24, 12);
      ctx.fill();
      ctx.fillStyle = '#8899aa';
      ctx.fillText(`ROUND ${planningRound}`, midX, midY + 4);
    }

    // Cleanup dead texts
    while (planningTexts.length > 0 && planningTexts[0].alpha <= 0) {
      planningTexts.shift();
    }
  }

  // --- Game Loop ---
  function gameLoop(timestamp: number = 0) {
    animFrame = requestAnimationFrame(gameLoop);
    if (!visible) return;

    const frameDelta = timestamp - lastFrameTime;
    if (frameDelta < FRAME_INTERVAL) return;
    lastFrameTime = timestamp - (frameDelta % FRAME_INTERVAL);

    const dt = Math.min((timestamp - lastTime) / 1000, 0.1);
    lastTime = timestamp;
    frameCount++;

    if (!ctx || canvasW === 0) return;

    if (frameCount % 90 === 0) syncAgentsFromTasks();

    // Background
    ensureBgCanvas();
    if (bgCanvas) ctx.drawImage(bgCanvas, 0, 0);

    drawMotes(ctx, motes, dt);

    // Planning beam (behind orbs)
    drawPlanningBeam(dt);

    for (let i = 0; i < ROOMS.length; i++) {
      drawRoom(ROOMS[i], i);
      drawRoomSubAgents(ROOMS[i], i);
    }

    // Planning text panels (near orbs)
    drawPlanningTexts(dt);

    drawParticles(ctx, particles, dt);

    for (let i = bubbles.length - 1; i >= 0; i--) {
      bubbles[i].timer -= dt;
      if (bubbles[i].timer < -0.5) bubbles.splice(i, 1);
    }
    drawBubbles({ ctx, bubbles, rooms: ROOMS, roomCount: ROOMS.length, canvasW, canvasH, dt });

    // Update agent state timers
    for (const id in agents) {
      const a = agents[id];
      if (a.stateTimer > 0) {
        a.stateTimer -= dt;
        if (a.stateTimer <= 0) {
          a.stateTimer = 0;
          let hasTask = false;
          try { hasTask = $tasks.some(t => t.target === id && t.status === 'running'); } catch { /* */ }
          a.state = hasTask ? 'working' : 'idle';
        }
      }
    }
  }

  // --- Mouse interaction ---
  function handleMouseMove(e: MouseEvent) {
    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;

    hoveredRoom = null;
    hoveredAgent = null;
    tooltip = null;

    for (let i = 0; i < ROOMS.length; i++) {
      const r = getRoomRect(i, ROOMS.length, canvasW, canvasH);
      if (mx >= r.x && mx < r.x + r.w && my >= r.y && my < r.y + r.h) {
        hoveredRoom = ROOMS[i].id;

        const center = getOrbCenter(i, ROOMS.length, canvasW, canvasH);
        const orbR = getOrbRadius(i, ROOMS.length, canvasW, canvasH);
        const time = performance.now() / 1000;
        const floatX = Math.sin(time * 0.7 + i * 1.5) * 4;
        const floatY = Math.cos(time * 0.5 + i * 1.2) * 3;
        const cx = center.x + floatX;
        const cy = center.y + floatY;
        const dist = Math.hypot(mx - cx, my - cy);
        if (dist < orbR * 1.5) {
          hoveredAgent = ROOMS[i].id;
          canvas.style.cursor = 'pointer';
        } else {
          canvas.style.cursor = 'default';
          const machine = Object.values($machines).find(m => m.id === ROOMS[i].id);
          if (machine) {
            tooltip = {
              x: mx, y: my,
              lines: [
                machine.name,
                `OS: ${machine.os}`,
                `Role: ${machine.role}`,
                machine.gpu ? `GPU: ${machine.gpu}` : '',
                machine.host === 'local' ? 'Local' : `Host: ${machine.host}`,
              ].filter(Boolean),
            };
          }
        }
        break;
      }
    }

    if (!hoveredRoom) canvas.style.cursor = 'default';
  }

  function handleClick() {
    if (hoveredAgent) {
      const roomIndex = ROOMS.findIndex(r => r.id === hoveredAgent);
      if (roomIndex >= 0) {
        const center = getOrbCenter(roomIndex, ROOMS.length, canvasW, canvasH);
        popup = popup?.machineId === hoveredAgent ? null : {
          machineId: hoveredAgent,
          x: center.x,
          y: center.y - getOrbRadius(roomIndex, ROOMS.length, canvasW, canvasH) - 40,
        };
      }
    } else {
      popup = null;
    }
  }

  function handleMouseLeave() {
    hoveredRoom = null;
    hoveredAgent = null;
    tooltip = null;
    canvas.style.cursor = 'default';
  }

  function formatElapsed(secs: number): string {
    if (secs < 60) return `${secs}s`;
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}m ${s}s`;
  }
</script>

<div class="pixel-rooms">
  <canvas
    bind:this={canvas}
    aria-label={t('pixel.canvasLabel')}
    tabindex="-1"
    onmousemove={handleMouseMove}
    onclick={handleClick}
    onmouseleave={handleMouseLeave}
  ></canvas>

  {#if tooltip}
    <div class="pr-tooltip" style="left:{tooltip.x + 12}px;top:{tooltip.y + 12}px">
      {#each tooltip.lines as line}
        <div>{line}</div>
      {/each}
    </div>
  {/if}

  {#if popup}
    {@const p = popup}
    {@const agent = agents[p.machineId]}
    {@const room = ROOMS.find(r => r.id === p.machineId)}
    <div
      class="pr-popup"
      style="left:{p.x}px;top:{Math.max(10, p.y - 50)}px"
    >
      <div class="pr-popup-header" style="color:{room?.primary || '#00d4ff'}">{p.machineId.toUpperCase()}</div>
      <div class="pr-popup-row">
        <span class="pr-popup-label">Estado</span>
        <span class="pr-popup-val pr-state-{agent?.state || 'idle'}">{agent?.state || 'idle'}</span>
      </div>
      {#if agent?.taskPrompt}
        <div class="pr-popup-row">
          <span class="pr-popup-label">Tarea</span>
          <span class="pr-popup-val">{agent.taskPrompt.slice(0, 50)}</span>
        </div>
        <div class="pr-popup-row">
          <span class="pr-popup-label">Tiempo</span>
          <span class="pr-popup-val">{formatElapsed(agent.elapsed)}</span>
        </div>
      {/if}
      <button class="pr-popup-close" onclick={() => popup = null}>x</button>
    </div>
  {/if}
</div>

<style>
  .pixel-rooms {
    flex: 1;
    position: relative;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: #020810;
  }
  .pixel-rooms canvas {
    width: 100%;
    height: 100%;
    display: block;
  }
  .pr-tooltip {
    position: absolute;
    background: rgba(4, 12, 24, 0.92);
    border: 1px solid rgba(255,255,255,0.08);
    border-radius: 6px;
    padding: 8px 12px;
    font-family: var(--font-mono);
    font-size: 9px;
    color: #8899aa;
    pointer-events: none;
    z-index: 10;
    line-height: 1.6;
    white-space: nowrap;
    backdrop-filter: blur(8px);
  }
  .pr-tooltip div:first-child {
    color: #c8d8e8;
    font-weight: 600;
    font-family: var(--font-display);
    font-size: 10px;
    margin-bottom: 2px;
  }
  .pr-popup {
    position: absolute;
    background: rgba(4, 12, 24, 0.92);
    border: 1px solid rgba(255,255,255,0.08);
    border-radius: 8px;
    padding: 12px 16px;
    min-width: 180px;
    transform: translateX(-50%);
    z-index: 20;
    box-shadow: 0 8px 32px rgba(0,0,0,0.6);
    backdrop-filter: blur(12px);
  }
  .pr-popup-header {
    font-family: var(--font-display);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 1.5px;
    margin-bottom: 8px;
  }
  .pr-popup-row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: 9px;
    padding: 2px 0;
  }
  .pr-popup-label {
    color: #4a5a6a;
    font-family: var(--font-display);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .pr-popup-val {
    color: #8899aa;
    text-align: right;
    max-width: 140px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pr-state-idle { color: #4a5a6a; }
  .pr-state-working { color: #4ade80; }
  .pr-state-error { color: #ff3355; }
  .pr-state-completing { color: #fbbf24; }
  .pr-popup-close {
    position: absolute;
    top: 6px;
    right: 8px;
    background: none;
    border: none;
    color: #3a4a5a;
    cursor: pointer;
    font-size: 14px;
    padding: 4px 8px;
  }
  .pr-popup-close:hover { color: #8899aa; }
</style>
