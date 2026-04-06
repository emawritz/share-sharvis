<script lang="ts">
  import { onMount } from 'svelte';
  import type { PlanningMessage, Activity } from '../types';

  let {
    messages = [],
    currentSpeaker = '',
    currentRound = 0,
    phase = 'planning',
    activity = [],
    elapsed = 0,
  }: {
    messages: PlanningMessage[];
    currentSpeaker: string;
    currentRound: number;
    phase: string;
    activity: Activity[];
    elapsed: number;
  } = $props();

  let canvas: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D;
  let canvasW = $state(900);
  let canvasH = $state(500);
  let animFrame = 0;
  let frameCount = 0;
  let lastTime = 0;
  const FPS = 30;
  const FRAME_INTERVAL = 1000 / FPS;
  let lastFrameTime = 0;

  const ATLAS = { id: 'atlas', name: 'ATLAS', primary: '#00BCD4', core: '#80DEEA', glow: 'rgba(0,188,212,' };
  const PIXEL = { id: 'pixel', name: 'PIXEL', primary: '#4CAF50', core: '#A5D6A7', glow: 'rgba(76,175,80,' };

  interface StreamParticle {
    t: number; speed: number; size: number; color: string; alpha: number;
    offset: number; wave: number;
  }
  interface BurstParticle {
    x: number; y: number; vx: number; vy: number;
    life: number; maxLife: number; color: string; size: number;
  }

  let streamParticles: StreamParticle[] = [];
  let burstParticles: BurstParticle[] = [];
  let prevSpeaker = '';
  let prevMsgCount = 0;
  let beamPower = 0.6; // always-on base level during planning
  let bgStars: { x: number; y: number; r: number; a: number }[] = [];

  function ac() { return { x: canvasW * 0.28, y: canvasH * 0.42 }; }
  function pc() { return { x: canvasW * 0.72, y: canvasH * 0.42 }; }
  function oR() { return Math.min(canvasW, canvasH) * 0.14; }

  onMount(() => {
    const rawCtx = canvas.getContext('2d');
    if (!rawCtx) return;
    ctx = rawCtx;
    for (let i = 0; i < 80; i++) {
      bgStars.push({ x: Math.random(), y: Math.random(), r: 0.3 + Math.random() * 1.2, a: 0.15 + Math.random() * 0.5 });
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
    return () => { cancelAnimationFrame(animFrame); ro.disconnect(); };
  });

  // React to speaker changes & new messages
  $effect(() => {
    if (currentSpeaker && currentSpeaker !== prevSpeaker) {
      const center = currentSpeaker === 'atlas' ? ac() : pc();
      const color = currentSpeaker === 'atlas' ? ATLAS.primary : PIXEL.primary;
      spawnBurst(center.x, center.y, color, 40);
      beamPower = 1.0;
      prevSpeaker = currentSpeaker;
    }
    if (messages.length > prevMsgCount && messages.length > 0) {
      const lastMsg = messages[messages.length - 1];
      const fromAtlas = lastMsg.sender === 'atlas';
      // Golden burst of stream particles on new message
      const goldenColors = ['#FFD740', '#FF8C00', '#FFA000', '#FFEE58', '#FF6D00'];
      for (let i = 0; i < 30; i++) {
        streamParticles.push({
          t: fromAtlas ? 0 : 1,
          speed: (0.004 + Math.random() * 0.009) * (fromAtlas ? 1 : -1),
          size: 2 + Math.random() * 4,
          color: goldenColors[Math.floor(Math.random() * goldenColors.length)],
          alpha: 0.7 + Math.random() * 0.3,
          offset: (Math.random() - 0.5) * 50,
          wave: Math.random() * Math.PI * 2,
        });
      }
      beamPower = 1.0;
      prevMsgCount = messages.length;
    }
  });

  function spawnBurst(x: number, y: number, color: string, count: number) {
    for (let i = 0; i < count; i++) {
      const angle = Math.random() * Math.PI * 2;
      const speed = 2 + Math.random() * 6;
      burstParticles.push({
        x, y, vx: Math.cos(angle) * speed, vy: Math.sin(angle) * speed,
        life: 1, maxLife: 0.5 + Math.random() * 1.0,
        color, size: 1.5 + Math.random() * 3.5,
      });
    }
  }

  function gameLoop(timestamp: number = 0) {
    animFrame = requestAnimationFrame(gameLoop);
    const frameDelta = timestamp - lastFrameTime;
    if (frameDelta < FRAME_INTERVAL) return;
    lastFrameTime = timestamp - (frameDelta % FRAME_INTERVAL);
    const dt = Math.min((timestamp - lastTime) / 1000, 0.1);
    lastTime = timestamp;
    frameCount++;
    if (!ctx || canvasW === 0) return;

    beamPower = Math.max(0.6, beamPower - dt * 0.3); // decays to 0.6 base

    const t = frameCount * 0.015;

    drawBg(t);
    drawBeam(t, dt);
    drawStreamParticles(dt, t);
    drawOrb(ac(), ATLAS, currentSpeaker === 'atlas', t);
    drawOrb(pc(), PIXEL, currentSpeaker === 'pixel', t);
    drawBurstParticles(dt);
    drawRound(t);
    drawActivity();
    drawLastMessages(t);
  }

  function drawBg(t: number) {
    const grad = ctx.createLinearGradient(0, 0, 0, canvasH);
    grad.addColorStop(0, '#010610');
    grad.addColorStop(0.5, '#030a16');
    grad.addColorStop(1, '#050d1c');
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, canvasW, canvasH);

    // Stars with twinkle
    for (const s of bgStars) {
      ctx.globalAlpha = s.a * (0.4 + Math.sin(t * 1.5 + s.x * 20) * 0.4);
      ctx.fillStyle = '#c8d8f0';
      ctx.beginPath();
      ctx.arc(s.x * canvasW, s.y * canvasH, s.r, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;

    // Large nebula glows
    for (const [orb, center] of [[ATLAS, ac()], [PIXEL, pc()]] as const) {
      const nebR = oR() * 5;
      const nebGrad = ctx.createRadialGradient(center.x, center.y, 0, center.x, center.y, nebR);
      nebGrad.addColorStop(0, orb.glow + '0.18)');
      nebGrad.addColorStop(0.3, orb.glow + '0.06)');
      nebGrad.addColorStop(1, 'transparent');
      ctx.fillStyle = nebGrad;
      ctx.fillRect(0, 0, canvasW, canvasH);
    }

    // Central glow when beam is active
    if (beamPower > 0.3) {
      const midX = (ac().x + pc().x) / 2;
      const midY = (ac().y + pc().y) / 2;
      const glow = ctx.createRadialGradient(midX, midY, 0, midX, midY, oR() * 3);
      glow.addColorStop(0, `rgba(255,255,255,${(beamPower - 0.3) * 0.08})`);
      glow.addColorStop(1, 'transparent');
      ctx.fillStyle = glow;
      ctx.fillRect(0, 0, canvasW, canvasH);
    }
  }

  // Persistent arc seeds — regenerated each frame for flicker
  let arcSeeds: number[][] = [];

  function buildArc(sx: number, sy: number, ex: number, ey: number, segs: number, amp: number): [number,number][] {
    const pts: [number,number][] = [[sx, sy]];
    for (let i = 1; i < segs; i++) {
      const f = i / segs;
      const bx = sx + (ex - sx) * f;
      const by = sy + (ey - sy) * f;
      // Envelope: zero at ends, max at center
      const env = Math.sin(f * Math.PI);
      pts.push([bx + (Math.random() - 0.5) * amp * 0.4, by + (Math.random() - 0.5) * amp * env]);
    }
    pts.push([ex, ey]);
    return pts;
  }

  function strokeArc(pts: [number,number][], color: string, width: number, alpha: number, blur: number) {
    ctx.save();
    ctx.globalAlpha = alpha;
    ctx.strokeStyle = color;
    ctx.lineWidth = width;
    ctx.shadowColor = color;
    ctx.shadowBlur = blur;
    ctx.beginPath();
    ctx.moveTo(pts[0][0], pts[0][1]);
    for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i][0], pts[i][1]);
    ctx.stroke();
    ctx.restore();
  }

  function drawBeam(t: number, _dt: number) {
    const a = ac(), p = pc(), r = oR();
    const sx = a.x + r * 0.9;
    const ex = p.x - r * 0.9;
    const midY = (a.y + p.y) / 2;
    const amp = canvasH * 0.28 * beamPower; // large vertical sweep

    // Soft golden glow fill between orbs
    const midX = (sx + ex) / 2;
    const glow = ctx.createRadialGradient(midX, midY, 0, midX, midY, (ex - sx) * 0.55);
    glow.addColorStop(0, `rgba(255,160,0,${0.12 * beamPower})`);
    glow.addColorStop(0.5, `rgba(200,80,0,${0.05 * beamPower})`);
    glow.addColorStop(1, 'transparent');
    ctx.fillStyle = glow;
    ctx.fillRect(0, 0, canvasW, canvasH);

    // Draw 3 main dramatic arcs — each recomputed each frame for flicker
    const arcDefs = [
      { segs: 22, amp: amp,        color: '#FF8C00', w: 2.5, alpha: beamPower * 0.7,  blur: 20 },
      { segs: 18, amp: amp * 0.65, color: '#FFD740', w: 1.5, alpha: beamPower * 0.55, blur: 14 },
      { segs: 28, amp: amp * 1.2,  color: '#FFA000', w: 1.0, alpha: beamPower * 0.4,  blur: 10 },
    ];
    for (const d of arcDefs) {
      const pts = buildArc(sx, midY, ex, midY, d.segs, d.amp);
      // Outer glow pass
      strokeArc(pts, d.color, d.w + 6, d.alpha * 0.15, d.blur * 2);
      // Main arc
      strokeArc(pts, d.color, d.w, d.alpha, d.blur);
      // Branch off the first arc
      if (d.segs === 22 && Math.random() < 0.4) {
        const branchIdx = 4 + Math.floor(Math.random() * (pts.length - 8));
        const [bx, by] = pts[branchIdx];
        const angle = -Math.PI / 4 + (Math.random() - 0.5) * 0.8;
        const len = amp * (0.3 + Math.random() * 0.5);
        const bEnd: [number,number] = [bx + Math.cos(angle) * len, by + Math.sin(angle) * len];
        const bpts = buildArc(bx, by, bEnd[0], bEnd[1], 6, len * 0.25);
        strokeArc(bpts, '#FFCC02', 0.8, beamPower * 0.35, 8);
      }
    }

    // Hot white core spine — thin, very bright
    if (beamPower > 0.55) {
      const spine = buildArc(sx, midY, ex, midY, 30, amp * 0.15);
      strokeArc(spine, '#FFFFFF', 0.7, (beamPower - 0.4) * 0.9, 6);
    }

    // Spawn golden stream particles (sparse)
    if (phase === 'planning' && frameCount % 4 === 0) {
      const fromAtlas = Math.random() > 0.5;
      streamParticles.push({
        t: fromAtlas ? 0 : 1,
        speed: (0.005 + Math.random() * 0.008) * (fromAtlas ? 1 : -1),
        size: 2 + Math.random() * 3.5,
        color: ['#FFD740','#FF8C00','#FFA000'][Math.floor(Math.random() * 3)],
        alpha: 0.7 + Math.random() * 0.3,
        offset: (Math.random() - 0.5) * 20,
        wave: Math.random() * Math.PI * 2,
      });
    }
  }

  function drawStreamParticles(dt: number, t: number) {
    const a = ac(), p = pc(), r = oR();
    const startX = a.x + r * 0.8;
    const endX = p.x - r * 0.8;
    const midY = (a.y + p.y) / 2;

    for (let i = streamParticles.length - 1; i >= 0; i--) {
      const sp = streamParticles[i];
      sp.t += sp.speed;
      sp.alpha -= 0.005;
      if (sp.t < -0.05 || sp.t > 1.05 || sp.alpha <= 0) {
        streamParticles.splice(i, 1);
        continue;
      }
      const x = startX + (endX - startX) * sp.t;
      const waveY = Math.sin(sp.t * Math.PI * 2 + t * 2 + sp.wave) * 8 + sp.offset * (1 - Math.abs(sp.t - 0.5) * 1.5);
      const y = midY + waveY;

      // Glow
      ctx.globalAlpha = sp.alpha * 0.4;
      const grd = ctx.createRadialGradient(x, y, 0, x, y, sp.size * 4);
      grd.addColorStop(0, sp.color);
      grd.addColorStop(1, 'transparent');
      ctx.fillStyle = grd;
      ctx.beginPath();
      ctx.arc(x, y, sp.size * 4, 0, Math.PI * 2);
      ctx.fill();

      // Core
      ctx.globalAlpha = sp.alpha;
      ctx.fillStyle = '#ffffff';
      ctx.beginPath();
      ctx.arc(x, y, sp.size * 0.6, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = sp.color;
      ctx.beginPath();
      ctx.arc(x, y, sp.size, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
    if (streamParticles.length > 120) streamParticles.splice(0, streamParticles.length - 120);
  }

  function drawOrb(center: { x: number; y: number }, orb: typeof ATLAS, isSpeaking: boolean, t: number) {
    const r = oR();
    const pulse = isSpeaking ? Math.sin(t * 3) * 0.18 + 0.15 : Math.sin(t * 0.8) * 0.03;
    const orbR = r * (1.0 + pulse * 0.4);
    const idx = orb === ATLAS ? 0 : 3;
    const floatX = Math.sin(t * 0.7 + idx) * 6;
    const floatY = Math.cos(t * 0.5 + idx * 0.7) * 5;
    const cx = center.x + floatX;
    const cy = center.y + floatY;

    // Outer glow - bigger and more intense when speaking
    const glowR = orbR * (isSpeaking ? 3.5 : 2.5);
    const glowAlpha = isSpeaking ? 0.2 : 0.08;
    const outerGrad = ctx.createRadialGradient(cx, cy, orbR * 0.3, cx, cy, glowR);
    outerGrad.addColorStop(0, orb.glow + `${glowAlpha})`);
    outerGrad.addColorStop(0.5, orb.glow + `${glowAlpha * 0.3})`);
    outerGrad.addColorStop(1, 'transparent');
    ctx.fillStyle = outerGrad;
    ctx.beginPath();
    ctx.arc(cx, cy, glowR, 0, Math.PI * 2);
    ctx.fill();

    // Energy ring when speaking
    if (isSpeaking) {
      const ringR = orbR * (1.3 + Math.sin(t * 4) * 0.15);
      ctx.globalAlpha = 0.15 + Math.sin(t * 3) * 0.08;
      ctx.strokeStyle = orb.primary;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(cx, cy, ringR, 0, Math.PI * 2);
      ctx.stroke();

      // Second ring
      const ringR2 = orbR * (1.6 + Math.cos(t * 3) * 0.1);
      ctx.globalAlpha = 0.08;
      ctx.beginPath();
      ctx.arc(cx, cy, ringR2, 0, Math.PI * 2);
      ctx.stroke();
      ctx.globalAlpha = 1;
    }

    // Body
    const bodyGrad = ctx.createRadialGradient(cx - orbR * 0.25, cy - orbR * 0.25, orbR * 0.05, cx, cy, orbR);
    bodyGrad.addColorStop(0, orb.core + 'ee');
    bodyGrad.addColorStop(0.3, orb.primary + 'bb');
    bodyGrad.addColorStop(0.7, orb.primary + '55');
    bodyGrad.addColorStop(1, orb.primary + '0a');
    ctx.fillStyle = bodyGrad;
    ctx.beginPath();
    ctx.arc(cx, cy, orbR, 0, Math.PI * 2);
    ctx.fill();

    // Membrane
    const memAlpha = isSpeaking ? 0.6 + Math.sin(t * 4) * 0.2 : 0.3;
    ctx.strokeStyle = orb.primary + (Math.floor(memAlpha * 255).toString(16).padStart(2, '0'));
    ctx.lineWidth = isSpeaking ? 2.5 : 1.5;
    ctx.beginPath();
    ctx.arc(cx, cy, orbR, 0, Math.PI * 2);
    ctx.stroke();

    // Cytoplasm motion (always on, stronger when speaking)
    ctx.save();
    ctx.beginPath();
    ctx.arc(cx, cy, orbR * 0.9, 0, Math.PI * 2);
    ctx.clip();
    ctx.globalAlpha = isSpeaking ? 0.2 : 0.08;
    ctx.strokeStyle = orb.core;
    ctx.lineWidth = isSpeaking ? 1.2 : 0.6;
    const numLines = isSpeaking ? 5 : 3;
    for (let i = 0; i < numLines; i++) {
      const angle = t * (0.7 + i * 0.25) + i * 2.1;
      const sx = cx + Math.cos(angle) * orbR * 0.4;
      const sy = cy + Math.sin(angle) * orbR * 0.4;
      const ex = cx + Math.cos(angle + 1.8) * orbR * 0.75;
      const ey = cy + Math.sin(angle + 1.8) * orbR * 0.75;
      ctx.beginPath();
      ctx.moveTo(sx, sy);
      ctx.quadraticCurveTo(
        cx + Math.sin(angle * 1.3) * orbR * 0.35,
        cy + Math.cos(angle * 1.1) * orbR * 0.35,
        ex, ey
      );
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
    ctx.restore();

    // Specular
    const specGrad = ctx.createRadialGradient(cx - orbR * 0.3, cy - orbR * 0.3, 0, cx - orbR * 0.15, cy - orbR * 0.15, orbR * 0.6);
    specGrad.addColorStop(0, 'rgba(255,255,255,0.3)');
    specGrad.addColorStop(0.5, 'rgba(255,255,255,0.06)');
    specGrad.addColorStop(1, 'transparent');
    ctx.fillStyle = specGrad;
    ctx.beginPath();
    ctx.arc(cx, cy, orbR, 0, Math.PI * 2);
    ctx.fill();

    // Energy tendrils when speaking
    if (isSpeaking) {
      ctx.globalAlpha = 0.12;
      ctx.strokeStyle = orb.primary;
      ctx.lineWidth = 1;
      for (let i = 0; i < 6; i++) {
        const angle = t * 0.8 + i * (Math.PI * 2 / 6);
        const len = orbR * (1.3 + Math.sin(t * 3 + i * 2) * 0.4);
        const ex = cx + Math.cos(angle) * len;
        const ey = cy + Math.sin(angle) * len;
        const cpx = cx + Math.cos(angle + 0.3) * orbR * 0.9;
        const cpy = cy + Math.sin(angle + 0.3) * orbR * 0.9;
        ctx.beginPath();
        ctx.moveTo(cx + Math.cos(angle) * orbR * 0.8, cy + Math.sin(angle) * orbR * 0.8);
        ctx.quadraticCurveTo(cpx, cpy, ex, ey);
        ctx.stroke();
      }
      ctx.globalAlpha = 1;

      // Emit motes
      if (frameCount % 6 === 0) {
        const angle = Math.random() * Math.PI * 2;
        const dist = orbR * (1 + Math.random() * 0.5);
        burstParticles.push({
          x: cx + Math.cos(angle) * dist, y: cy + Math.sin(angle) * dist,
          vx: Math.cos(angle) * 0.5, vy: Math.sin(angle) * 0.5,
          life: 1, maxLife: 1.5 + Math.random(), color: orb.primary, size: 1 + Math.random() * 2,
        });
      }
    }

    // Label
    ctx.font = '700 13px "Chakra Petch", sans-serif';
    ctx.textAlign = 'center';
    ctx.fillStyle = orb.primary;
    ctx.globalAlpha = isSpeaking ? 1 : 0.6;
    ctx.fillText(orb.name, cx, cy + orbR + 26);

    if (isSpeaking) {
      ctx.font = '600 10px "Chakra Petch", sans-serif';
      ctx.globalAlpha = 0.5 + Math.sin(t * 4) * 0.3;
      ctx.fillText('THINKING...', cx, cy + orbR + 42);
    }
    ctx.globalAlpha = 1;
  }

  function drawBurstParticles(dt: number) {
    for (let i = burstParticles.length - 1; i >= 0; i--) {
      const bp = burstParticles[i];
      bp.x += bp.vx;
      bp.y += bp.vy;
      bp.vy += 0.015;
      bp.vx *= 0.995;
      bp.life -= dt / bp.maxLife;
      if (bp.life <= 0) { burstParticles.splice(i, 1); continue; }

      // Glow
      ctx.globalAlpha = Math.max(0, bp.life) * 0.3;
      const grd = ctx.createRadialGradient(bp.x, bp.y, 0, bp.x, bp.y, bp.size * 3);
      grd.addColorStop(0, bp.color);
      grd.addColorStop(1, 'transparent');
      ctx.fillStyle = grd;
      ctx.beginPath();
      ctx.arc(bp.x, bp.y, bp.size * 3, 0, Math.PI * 2);
      ctx.fill();

      // Core
      ctx.globalAlpha = Math.max(0, bp.life) * 0.8;
      ctx.fillStyle = bp.color;
      ctx.beginPath();
      ctx.arc(bp.x, bp.y, bp.size * bp.life, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
    if (burstParticles.length > 150) burstParticles.splice(0, burstParticles.length - 150);
  }

  function drawRound(t: number) {
    if (currentRound <= 0) return;
    const midX = canvasW / 2;
    const midY = canvasH * 0.1;

    ctx.font = '700 11px "Chakra Petch", sans-serif';
    ctx.textAlign = 'center';

    // Badge background with subtle glow
    ctx.fillStyle = 'rgba(255,255,255,0.06)';
    ctx.beginPath();
    ctx.roundRect(midX - 50, midY - 14, 100, 28, 14);
    ctx.fill();
    ctx.strokeStyle = 'rgba(255,255,255,0.08)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.roundRect(midX - 50, midY - 14, 100, 28, 14);
    ctx.stroke();

    ctx.fillStyle = '#aabbcc';
    ctx.fillText(`ROUND ${currentRound}`, midX, midY + 4);

    if (elapsed > 0) {
      ctx.font = '400 9px "IBM Plex Mono", monospace';
      ctx.fillStyle = '#556677';
      const m = Math.floor(elapsed / 60);
      const s = elapsed % 60;
      ctx.fillText(m > 0 ? `${m}m ${s}s` : `${s}s`, midX, midY + 22);
    }
  }

  function drawActivity() {
    if (activity.length === 0) return;
    const y = canvasH - 50;
    const recent = activity.slice(-3);
    ctx.font = '400 9px "IBM Plex Mono", monospace';
    ctx.textAlign = 'center';
    for (let i = 0; i < recent.length; i++) {
      const item = recent[i];
      const alpha = 0.2 + (i / recent.length) * 0.5;
      ctx.globalAlpha = alpha;
      const ly = y + i * 14;
      if (item.type === 'tool') {
        ctx.fillStyle = currentSpeaker === 'atlas' ? ATLAS.primary : PIXEL.primary;
        ctx.fillText(`> ${item.name}  ${(item.detail || '').substring(0, 60)}`, canvasW / 2, ly);
      } else {
        ctx.fillStyle = '#5a6a7a';
        ctx.fillText((item.content || '').replace(/\n/g, ' ').substring(0, 80), canvasW / 2, ly);
      }
    }
    ctx.globalAlpha = 1;
  }

  /** Word-wrap text to fit within maxWidth pixels */
  function wrapText(text: string, maxWidth: number, fontSize: number): string[] {
    const words = text.replace(/\s+/g, ' ').trim().split(' ');
    const lines: string[] = [];
    let current = '';
    // Approximate char width at given font size (monospace ~0.6x)
    const charW = fontSize * 0.6;
    const maxChars = Math.floor(maxWidth / charW);
    for (const word of words) {
      const test = current ? current + ' ' + word : word;
      if (test.length > maxChars && current) {
        lines.push(current);
        current = word;
      } else {
        current = test;
      }
    }
    if (current) lines.push(current);
    return lines;
  }

  function drawMessagePanel(
    msg: { sender: string; round: number; content: string },
    isAtlas: boolean,
    fade: number,
    t: number,
    panelY: number
  ) {
    const a = ac(), p = pc(), r = oR();
    const orbColor = isAtlas ? ATLAS.primary : PIXEL.primary;
    const orbColorDim = isAtlas ? ATLAS.glow : PIXEL.glow;

    const margin = 18;
    const maxPanelW = Math.min(canvasW * 0.32, 300);
    // Atlas panel: left side; Pixel panel: right side
    const px = isAtlas ? margin : canvasW - margin - maxPanelW;

    const headerH = 28;
    const lineH = 17;
    const fontSize = 11;
    const padX = 14;
    const contentW = maxPanelW - padX * 2;

    const rawText = msg.content.replace(/\n+/g, ' ').trim();
    const lines = wrapText(rawText, contentW, fontSize);
    const shownLines = lines.slice(0, 5); // max 5 lines
    const panelH = headerH + shownLines.length * lineH + 16;

    ctx.save();
    ctx.globalAlpha = fade;

    // Outer glow halo
    const haloGrad = ctx.createLinearGradient(px, panelY, px + maxPanelW, panelY);
    haloGrad.addColorStop(0, orbColorDim + '0.06)');
    haloGrad.addColorStop(0.5, orbColorDim + '0.12)');
    haloGrad.addColorStop(1, orbColorDim + '0.04)');
    ctx.fillStyle = haloGrad;
    ctx.shadowColor = orbColor;
    ctx.shadowBlur = 12;
    ctx.beginPath();
    ctx.roundRect(px - 4, panelY - 4, maxPanelW + 8, panelH + 8, 10);
    ctx.fill();
    ctx.shadowBlur = 0;

    // Panel background
    const bgGrad = ctx.createLinearGradient(px, panelY, px, panelY + panelH);
    bgGrad.addColorStop(0, 'rgba(4, 10, 22, 0.96)');
    bgGrad.addColorStop(1, 'rgba(2, 6, 14, 0.92)');
    ctx.fillStyle = bgGrad;
    ctx.beginPath();
    ctx.roundRect(px, panelY, maxPanelW, panelH, 7);
    ctx.fill();

    // Border
    ctx.strokeStyle = orbColor + '50';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.roundRect(px, panelY, maxPanelW, panelH, 7);
    ctx.stroke();

    // Header strip
    const hdrGrad = ctx.createLinearGradient(px, panelY, px + maxPanelW, panelY);
    hdrGrad.addColorStop(0, orbColorDim + '0.35)');
    hdrGrad.addColorStop(1, 'transparent');
    ctx.fillStyle = hdrGrad;
    ctx.beginPath();
    ctx.roundRect(px, panelY, maxPanelW, headerH, [7, 7, 0, 0]);
    ctx.fill();

    // Left accent bar
    ctx.fillStyle = orbColor;
    ctx.shadowColor = orbColor;
    ctx.shadowBlur = 6;
    ctx.fillRect(px, panelY + 5, 3, panelH - 10);
    ctx.shadowBlur = 0;

    // Header text: sender + round
    ctx.font = '700 11px "Chakra Petch", monospace';
    ctx.textAlign = 'left';
    ctx.fillStyle = orbColor;
    ctx.globalAlpha = fade;
    ctx.shadowColor = orbColor;
    ctx.shadowBlur = 8;
    ctx.fillText(`${msg.sender.toUpperCase()}`, px + padX + 2, panelY + 18);
    ctx.shadowBlur = 0;

    // Round badge on right
    ctx.font = '600 9px "Chakra Petch", monospace';
    ctx.textAlign = 'right';
    ctx.fillStyle = orbColor + 'aa';
    ctx.fillText(`RONDA ${msg.round}`, px + maxPanelW - 10, panelY + 17);

    // Divider
    ctx.strokeStyle = orbColor + '28';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(px + padX, panelY + headerH);
    ctx.lineTo(px + maxPanelW - padX, panelY + headerH);
    ctx.stroke();

    // Content lines
    ctx.textAlign = 'left';
    ctx.font = `400 ${fontSize}px "IBM Plex Mono", monospace`;
    for (let li = 0; li < shownLines.length; li++) {
      // Fade last partial line if truncated
      const isLast = li === shownLines.length - 1 && lines.length > shownLines.length;
      ctx.globalAlpha = fade * (isLast ? 0.35 : (0.55 + li * 0.05));
      ctx.fillStyle = li === 0 ? '#cce8ff' : '#8baabb';
      ctx.fillText(shownLines[li], px + padX + 2, panelY + headerH + 13 + li * lineH);
    }

    // Truncation indicator
    if (lines.length > shownLines.length) {
      ctx.globalAlpha = fade * 0.3;
      ctx.font = '400 9px "IBM Plex Mono", monospace';
      ctx.fillStyle = orbColor;
      ctx.fillText(`+ ${lines.length - shownLines.length} más...`, px + padX + 2, panelY + headerH + 13 + shownLines.length * lineH);
    }

    ctx.restore();
  }

  function drawLastMessages(t: number) {
    if (messages.length === 0) return;

    const a = ac(), p = pc(), r = oR();

    // Show last atlas message and last pixel message separately
    const atlasMsg = [...messages].reverse().find(m => m.sender === 'atlas');
    const pixelMsg = [...messages].reverse().find(m => m.sender === 'pixel');
    const lastMsg = messages[messages.length - 1];

    const panelTopY = a.y - r * 0.6;

    if (atlasMsg) {
      const isLatest = atlasMsg === lastMsg;
      drawMessagePanel(atlasMsg, true, isLatest ? 0.92 : 0.45, t, panelTopY);
    }
    if (pixelMsg) {
      const isLatest = pixelMsg === lastMsg;
      drawMessagePanel(pixelMsg, false, isLatest ? 0.92 : 0.45, t, panelTopY);
    }
  }
</script>

<div class="planning-canvas-wrap">
  <canvas bind:this={canvas} tabindex="-1"></canvas>
</div>

<style>
  .planning-canvas-wrap {
    flex: 1;
    position: relative;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: #010610;
  }
  .planning-canvas-wrap canvas {
    width: 100%;
    height: 100%;
    display: block;
  }
</style>
