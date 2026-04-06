// Pure rendering functions for PixelRooms canvas
import type { RoomDef, AgentState, Bubble, Particle, Mote, ActivityItem } from './types';

// --- Background rendering ---

export interface BgStar {
  x: number; y: number; r: number; a: number; speed: number;
}

export function renderBackground(
  bgCtx: CanvasRenderingContext2D,
  rooms: RoomDef[],
  stars: BgStar[],
  canvasW: number, canvasH: number
) {
  const grad = bgCtx.createLinearGradient(0, 0, 0, canvasH);
  grad.addColorStop(0, '#020810');
  grad.addColorStop(0.4, '#040c18');
  grad.addColorStop(1, '#060e1e');
  bgCtx.fillStyle = grad;
  bgCtx.fillRect(0, 0, canvasW, canvasH);

  for (let i = 0; i < rooms.length; i++) {
    const center = getOrbCenter(i, rooms.length, canvasW, canvasH);
    const room = rooms[i];
    const nebR = getOrbRadius(i, rooms.length, canvasW, canvasH) * 4;
    const nebGrad = bgCtx.createRadialGradient(center.x, center.y, 0, center.x, center.y, nebR);
    nebGrad.addColorStop(0, room.glow);
    nebGrad.addColorStop(0.5, room.glow.replace('0.12', '0.04'));
    nebGrad.addColorStop(1, 'transparent');
    bgCtx.fillStyle = nebGrad;
    bgCtx.fillRect(0, 0, canvasW, canvasH);
  }

  for (const star of stars) {
    bgCtx.fillStyle = '#c8d8f0';
    bgCtx.globalAlpha = star.a * 0.7;
    bgCtx.beginPath();
    bgCtx.arc(star.x * canvasW, star.y * canvasH, star.r, 0, Math.PI * 2);
    bgCtx.fill();
  }
  bgCtx.globalAlpha = 1;

  for (let i = 1; i < rooms.length; i++) {
    const x = Math.floor(canvasW / rooms.length) * i;
    bgCtx.strokeStyle = 'rgba(255,255,255,0.04)';
    bgCtx.lineWidth = 1;
    bgCtx.beginPath();
    bgCtx.moveTo(x, 0);
    bgCtx.lineTo(x, canvasH - 72);
    bgCtx.stroke();
  }
}

// --- Orb state computation ---

export interface OrbVisuals {
  orbR: number;
  pulse: number;
  membrane: number;
  orbColor: string;
  orbCore: string;
  orbGlow: string;
  /** Whether to spawn motes this frame */
  shouldSpawnMotes: boolean;
  /** Mote color (if shouldSpawnMotes) */
  moteColor: string;
  /** Whether to also spawn particles this frame */
  shouldSpawnParticles: boolean;
  /** Particle colors (if shouldSpawnParticles) */
  particleColors: string[];
}

export function computeOrbVisuals(
  room: RoomDef, agState: string, isOffline: boolean,
  baseR: number, frameCount: number
): OrbVisuals {
  const t = frameCount * 0.015;
  let orbR = baseR;
  let pulse = 0;
  let membrane = 0.3;
  let orbColor = room.primary;
  let orbCore = room.core;
  let orbGlow = room.glow;
  let shouldSpawnMotes = false;
  let moteColor = '';
  let shouldSpawnParticles = false;
  let particleColors: string[] = [];

  if (isOffline) {
    orbR = baseR * 0.6;
    orbColor = '#2a3a4a'; orbCore = '#3a4a5a'; orbGlow = 'rgba(42,58,74,0.12)';
    membrane = 0.15;
  } else if (agState === 'working') {
    pulse = Math.sin(t * 2) * 0.15 + 0.1;
    orbR = baseR * (1.0 + pulse * 0.3);
    membrane = 0.5 + Math.sin(t * 3) * 0.15;
    if (frameCount % 20 === 0) { shouldSpawnMotes = true; moteColor = orbColor; }
  } else if (agState === 'error') {
    orbColor = '#ff3355'; orbCore = '#ff8899'; orbGlow = 'rgba(255,51,85,0.12)';
    pulse = Math.sin(t * 6) * 0.2;
    membrane = 0.6 + Math.sin(t * 8) * 0.2;
    if (frameCount % 24 === 0) { shouldSpawnMotes = true; moteColor = '#ff3355'; }
  } else if (agState === 'completing') {
    orbColor = '#00ff88'; orbCore = '#aaffcc'; orbGlow = 'rgba(0,255,136,0.12)';
    pulse = Math.sin(t * 4) * 0.25 + 0.15;
    membrane = 0.7;
    if (frameCount % 12 === 0) {
      shouldSpawnMotes = true; moteColor = '#00ff88';
      shouldSpawnParticles = true; particleColors = ['#00ff88', room.primary];
    }
  } else {
    pulse = Math.sin(t * 0.8) * 0.05;
    orbR = baseR * (0.85 + pulse);
    membrane = 0.2 + Math.sin(t) * 0.05;
  }

  return { orbR, pulse, membrane, orbColor, orbCore, orbGlow, shouldSpawnMotes, moteColor, shouldSpawnParticles, particleColors };
}

// --- Full room draw (pure rendering, no store access) ---

export interface DrawRoomParams {
  ctx: CanvasRenderingContext2D;
  room: RoomDef;
  index: number;
  isOffline: boolean;
  agState: string;
  agentCount: number;
  roomCount: number;
  canvasW: number;
  canvasH: number;
  frameCount: number;
}

export function drawRoomOrb(params: DrawRoomParams): OrbVisuals {
  const { ctx, room, index, isOffline, agState, agentCount, roomCount, canvasW, canvasH, frameCount } = params;
  const center = getOrbCenter(index, roomCount, canvasW, canvasH);
  const baseR = getOrbRadius(index, roomCount, canvasW, canvasH);
  const t = frameCount * 0.015;

  const vis = computeOrbVisuals(room, agState, isOffline, baseR, frameCount);

  const floatX = Math.sin(t * 0.7 + index * 1.5) * 4;
  const floatY = Math.cos(t * 0.5 + index * 1.2) * 3;
  const cx = center.x + floatX;
  const cy = center.y + floatY;

  drawOrb(ctx, cx, cy, vis.orbR, vis.orbColor, vis.orbCore, vis.orbGlow, vis.pulse, vis.membrane);

  if (!isOffline && agState === 'working') {
    drawCytoplasm(ctx, cx, cy, vis.orbR, vis.orbCore, t);
  }

  drawRoomLabel(ctx, room.name, center.x, center.y, vis.orbR, vis.orbColor, isOffline);

  if (agentCount > 0) {
    const r = getRoomRect(index, roomCount, canvasW, canvasH);
    drawAgentBadge(ctx, agentCount, vis.orbColor, r.x, r.w);
  }

  if (isOffline) drawOfflineZzz(ctx, cx, cy, vis.orbR, t);

  return vis;
}

// --- Geometry helpers ---

export function getRoomRect(index: number, roomCount: number, canvasW: number, canvasH: number) {
  const chatBarH = 72;
  const roomW = Math.floor(canvasW / roomCount);
  const roomH = canvasH - chatBarH;
  return { x: index * roomW, y: 0, w: roomW, h: roomH };
}

export function getOrbCenter(index: number, roomCount: number, canvasW: number, canvasH: number) {
  const r = getRoomRect(index, roomCount, canvasW, canvasH);
  return { x: r.x + r.w / 2, y: r.y + r.h * 0.45 };
}

export function getOrbRadius(index: number, roomCount: number, canvasW: number, canvasH: number): number {
  const r = getRoomRect(index, roomCount, canvasW, canvasH);
  return Math.min(r.w, r.h) * 0.18;
}

// --- Orb drawing ---

export function drawOrb(
  ctx: CanvasRenderingContext2D,
  cx: number, cy: number, radius: number,
  color: string, coreColor: string, glowColor: string,
  pulse: number, membrane: number
) {
  // Outer glow (large, soft)
  const outerR = radius * (2.5 + pulse * 0.3);
  const outerGrad = ctx.createRadialGradient(cx, cy, radius * 0.3, cx, cy, outerR);
  outerGrad.addColorStop(0, glowColor.replace('0.12', '0.08'));
  outerGrad.addColorStop(0.4, glowColor.replace('0.12', '0.03'));
  outerGrad.addColorStop(1, 'transparent');
  ctx.fillStyle = outerGrad;
  ctx.beginPath();
  ctx.arc(cx, cy, outerR, 0, Math.PI * 2);
  ctx.fill();

  // Inner body
  const bodyGrad = ctx.createRadialGradient(
    cx - radius * 0.25, cy - radius * 0.25, radius * 0.05,
    cx, cy, radius
  );
  bodyGrad.addColorStop(0, coreColor + 'dd');
  bodyGrad.addColorStop(0.3, color + 'aa');
  bodyGrad.addColorStop(0.7, color + '44');
  bodyGrad.addColorStop(1, color + '08');
  ctx.fillStyle = bodyGrad;
  ctx.beginPath();
  ctx.arc(cx, cy, radius, 0, Math.PI * 2);
  ctx.fill();

  // Membrane edge
  ctx.strokeStyle = color + (Math.floor(membrane * 255).toString(16).padStart(2, '0'));
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  ctx.arc(cx, cy, radius, 0, Math.PI * 2);
  ctx.stroke();

  // Specular highlight (top-left)
  const specGrad = ctx.createRadialGradient(
    cx - radius * 0.3, cy - radius * 0.3, 0,
    cx - radius * 0.15, cy - radius * 0.15, radius * 0.6
  );
  specGrad.addColorStop(0, 'rgba(255,255,255,0.25)');
  specGrad.addColorStop(0.5, 'rgba(255,255,255,0.05)');
  specGrad.addColorStop(1, 'transparent');
  ctx.fillStyle = specGrad;
  ctx.beginPath();
  ctx.arc(cx, cy, radius, 0, Math.PI * 2);
  ctx.fill();
}

// --- Internal motion lines (cytoplasm effect) ---

export function drawCytoplasm(
  ctx: CanvasRenderingContext2D,
  cx: number, cy: number, orbR: number,
  orbCore: string, t: number
) {
  ctx.save();
  ctx.beginPath();
  ctx.arc(cx, cy, orbR * 0.9, 0, Math.PI * 2);
  ctx.clip();
  ctx.globalAlpha = 0.12;
  ctx.strokeStyle = orbCore;
  ctx.lineWidth = 0.8;
  for (let i = 0; i < 3; i++) {
    const angle = t * (0.5 + i * 0.3) + i * 2.1;
    const sx = cx + Math.cos(angle) * orbR * 0.4;
    const sy = cy + Math.sin(angle) * orbR * 0.4;
    const ex = cx + Math.cos(angle + 1.5) * orbR * 0.7;
    const ey = cy + Math.sin(angle + 1.5) * orbR * 0.7;
    ctx.beginPath();
    ctx.moveTo(sx, sy);
    ctx.quadraticCurveTo(
      cx + Math.sin(angle * 1.3) * orbR * 0.3,
      cy + Math.cos(angle * 1.1) * orbR * 0.3,
      ex, ey
    );
    ctx.stroke();
  }
  ctx.globalAlpha = 1;
  ctx.restore();
}

// --- Offline ZZZ indicator ---

export function drawOfflineZzz(
  ctx: CanvasRenderingContext2D,
  cx: number, cy: number, orbR: number, t: number
) {
  ctx.globalAlpha = 0.3;
  ctx.fillStyle = '#4a5a6a';
  ctx.font = '600 14px "Chakra Petch", sans-serif';
  ctx.textAlign = 'center';
  const zzPhase = Math.sin(t) * 5;
  ctx.fillText('Z', cx + 15, cy - orbR - 5 + zzPhase);
  ctx.font = '600 10px "Chakra Petch", sans-serif';
  ctx.fillText('z', cx + 25, cy - orbR - 15 + zzPhase * 0.7);
  ctx.font = '600 7px "Chakra Petch", sans-serif';
  ctx.fillText('z', cx + 32, cy - orbR - 22 + zzPhase * 0.4);
  ctx.globalAlpha = 1;
}

// --- Room label ---

export function drawRoomLabel(
  ctx: CanvasRenderingContext2D,
  name: string, centerX: number, centerY: number,
  orbR: number, color: string, isOffline: boolean
) {
  ctx.font = '600 11px "Chakra Petch", sans-serif';
  ctx.textAlign = 'center';
  ctx.fillStyle = isOffline ? '#2a3a4a' : color;
  ctx.globalAlpha = 0.8;
  ctx.fillText(name, centerX, centerY + orbR + 22);
  ctx.globalAlpha = 1;
}

// --- Agent count badge ---

export function drawAgentBadge(
  ctx: CanvasRenderingContext2D,
  agentCount: number, orbColor: string,
  roomX: number, roomW: number
) {
  const badge = `${agentCount}`;
  ctx.font = '600 9px monospace';
  ctx.fillStyle = orbColor + '33';
  ctx.beginPath();
  ctx.arc(roomX + roomW - 18, 16, 10, 0, Math.PI * 2);
  ctx.fill();
  ctx.strokeStyle = orbColor + '66';
  ctx.lineWidth = 1;
  ctx.stroke();
  ctx.fillStyle = orbColor;
  ctx.textAlign = 'center';
  ctx.fillText(badge, roomX + roomW - 18, 20);
}

// --- Sub-agent orbs ---

export interface SubAgentParams {
  ctx: CanvasRenderingContext2D;
  room: RoomDef;
  center: { x: number; y: number };
  mainR: number;
  count: number;
  frameCount: number;
  isWorking: boolean;
}

export function drawSubAgents(params: SubAgentParams) {
  const { ctx, room, center, mainR, count, frameCount, isWorking } = params;
  if (count <= 0) return;

  const t = frameCount * 0.015;

  for (let i = 0; i < count; i++) {
    const orbitR = mainR * (1.8 + i * 0.35);
    const speed = (0.4 + i * 0.15) * (i % 2 === 0 ? 1 : -1);
    const angle = t * speed + (i * Math.PI * 2) / count;
    const subR = mainR * (0.2 + 0.05 * Math.sin(t * 2 + i));

    const sx = center.x + Math.cos(angle) * orbitR + Math.sin(t * 0.7 + i) * 3;
    const sy = center.y + Math.sin(angle) * orbitR * 0.6 + Math.cos(t * 0.5 + i) * 2;

    const subColor = room.primary;
    const subCore = room.core;

    // Subtle orbit trail
    if (isWorking) {
      ctx.globalAlpha = 0.04;
      ctx.strokeStyle = subColor;
      ctx.lineWidth = subR * 0.5;
      ctx.beginPath();
      ctx.arc(center.x, center.y, orbitR, angle - 0.5, angle, false);
      ctx.stroke();
      ctx.globalAlpha = 1;
    }

    // Draw sub-orb (simplified for perf)
    const subAlpha = isWorking ? 0.7 : 0.4;
    ctx.globalAlpha = subAlpha;
    const subGrad = ctx.createRadialGradient(sx - subR * 0.2, sy - subR * 0.2, 0, sx, sy, subR);
    subGrad.addColorStop(0, subCore);
    subGrad.addColorStop(0.6, subColor + '88');
    subGrad.addColorStop(1, subColor + '11');
    ctx.fillStyle = subGrad;
    ctx.beginPath();
    ctx.arc(sx, sy, subR, 0, Math.PI * 2);
    ctx.fill();
    // Membrane
    ctx.strokeStyle = subColor + '44';
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.globalAlpha = 1;

    // Connecting thread to main orb
    if (isWorking) {
      ctx.globalAlpha = 0.06;
      ctx.strokeStyle = room.primary;
      ctx.lineWidth = 0.5;
      ctx.beginPath();
      ctx.moveTo(center.x, center.y);
      ctx.lineTo(sx, sy);
      ctx.stroke();
      ctx.globalAlpha = 1;
    }
  }
}

// Returns the sub-agent positions that need mote spawning
export function getSubAgentMotePositions(
  room: RoomDef,
  center: { x: number; y: number },
  mainR: number,
  count: number,
  frameCount: number,
  isWorking: boolean
): { x: number; y: number; color: string }[] {
  if (!isWorking || count <= 0) return [];
  const t = frameCount * 0.015;
  const results: { x: number; y: number; color: string }[] = [];
  for (let i = 0; i < count; i++) {
    if (frameCount % 40 === i % 40) {
      const orbitR = mainR * (1.8 + i * 0.35);
      const speed = (0.4 + i * 0.15) * (i % 2 === 0 ? 1 : -1);
      const angle = t * speed + (i * Math.PI * 2) / count;
      const sx = center.x + Math.cos(angle) * orbitR + Math.sin(t * 0.7 + i) * 3;
      const sy = center.y + Math.sin(angle) * orbitR * 0.6 + Math.cos(t * 0.5 + i) * 2;
      results.push({ x: sx, y: sy, color: room.primary });
    }
  }
  return results;
}

// --- Motes ---

export function drawMotes(ctx: CanvasRenderingContext2D, motes: Mote[], _dt: number) {
  for (let i = motes.length - 1; i >= 0; i--) {
    const m = motes[i];
    m.x += m.vx;
    m.y += m.vy;
    m.alpha -= m.decay;
    m.r *= 0.998;

    if (m.alpha <= 0 || m.r < 0.2) {
      motes.splice(i, 1);
      continue;
    }

    ctx.globalAlpha = m.alpha * 0.6;
    ctx.fillStyle = m.color;
    ctx.beginPath();
    ctx.arc(m.x, m.y, m.r, 0, Math.PI * 2);
    ctx.fill();
  }
  ctx.globalAlpha = 1;
  if (motes.length > 40) motes.splice(0, motes.length - 40);
}

// --- Particles ---

export function drawParticles(ctx: CanvasRenderingContext2D, particles: Particle[], dt: number) {
  for (let i = particles.length - 1; i >= 0; i--) {
    const p = particles[i];
    p.x += p.vx;
    p.y += p.vy;
    p.vy += 0.03;
    p.life -= dt / p.maxLife;

    if (p.life <= 0) { particles.splice(i, 1); continue; }

    ctx.globalAlpha = Math.max(0, p.life) * 0.7;
    ctx.fillStyle = p.color;
    ctx.fillRect(Math.floor(p.x), Math.floor(p.y), Math.ceil(p.size), Math.ceil(p.size));
  }
  ctx.globalAlpha = 1;
}

// --- Bubbles ---

export interface BubbleDrawParams {
  ctx: CanvasRenderingContext2D;
  bubbles: Bubble[];
  rooms: RoomDef[];
  roomCount: number;
  canvasW: number;
  canvasH: number;
  dt: number;
}

export function drawBubbles(params: BubbleDrawParams) {
  const { ctx, bubbles, rooms, roomCount, canvasW, canvasH } = params;
  const activeBubbles = bubbles.filter(b => b.timer > 0);
  for (const b of activeBubbles) {
    const roomIndex = rooms.findIndex(r => r.id === b.machineId);
    if (roomIndex < 0) continue;
    const center = getOrbCenter(roomIndex, roomCount, canvasW, canvasH);
    const orbR = getOrbRadius(roomIndex, roomCount, canvasW, canvasH);

    const bx = center.x;
    const stackOffset = activeBubbles.filter(ab => ab.machineId === b.machineId && ab.timer > b.timer).length;
    const byRaw = center.y - orbR - 30 - stackOffset * 22;
    const alpha = Math.min(1, b.timer / 0.5);

    ctx.font = '500 9px "IBM Plex Mono", monospace';
    const textW = ctx.measureText(b.text).width;
    const padX = 8;
    const padY = 4;
    const bw = textW + padX * 2;
    const bh = 14 + padY * 2;
    const by = Math.max(bh + 4, byRaw);

    ctx.globalAlpha = alpha * 0.9;

    // Frosted glass bubble
    ctx.fillStyle = 'rgba(4,12,24,0.75)';
    ctx.strokeStyle = b.color + '66';
    ctx.lineWidth = 1;
    const rx = bx - bw / 2;
    const ry = by - bh;
    ctx.beginPath();
    ctx.roundRect(rx, ry, bw, bh, 6);
    ctx.fill();
    ctx.stroke();

    // Subtle glow behind bubble
    const bubGlow = ctx.createRadialGradient(bx, by - bh / 2, 0, bx, by - bh / 2, bw);
    bubGlow.addColorStop(0, b.color + '11');
    bubGlow.addColorStop(1, 'transparent');
    ctx.fillStyle = bubGlow;
    ctx.fillRect(rx - bw * 0.3, ry - bh * 0.3, bw * 1.6, bh * 1.6);

    // Text
    ctx.fillStyle = '#e0e8f0';
    ctx.textAlign = 'center';
    ctx.fillText(b.text, bx, by - padY - 3);
    ctx.globalAlpha = 1;
  }
}

// --- Activity log panel ---

export function drawActivityLog(
  ctx: CanvasRenderingContext2D,
  room: RoomDef, index: number,
  feed: ActivityItem[],
  roomCount: number, canvasW: number, canvasH: number
) {
  if (feed.length === 0) return;

  const r = getRoomRect(index, roomCount, canvasW, canvasH);
  const chatBarH = 72;
  const logH = Math.min(r.h * 0.28, 120);
  const logY = r.y + r.h - chatBarH - logH - 4;
  const logX = r.x + 8;
  const logW = r.w - 16;
  const lineH = 13;
  const maxLines = Math.floor(logH / lineH);

  // Translucent background
  ctx.fillStyle = 'rgba(2,6,14,0.6)';
  ctx.beginPath();
  ctx.roundRect(logX, logY, logW, logH, 6);
  ctx.fill();

  // Subtle border
  ctx.strokeStyle = room.primary + '18';
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.roundRect(logX, logY, logW, logH, 6);
  ctx.stroke();

  // Clip text
  ctx.save();
  ctx.beginPath();
  ctx.roundRect(logX, logY, logW, logH, 6);
  ctx.clip();

  const recent = feed.slice(-maxLines);
  ctx.textAlign = 'left';

  for (let i = 0; i < recent.length; i++) {
    const a = recent[i];
    const ly = logY + 11 + i * lineH;
    const fade = 0.3 + (i / recent.length) * 0.7;
    ctx.globalAlpha = fade;

    if (a.type === 'tool') {
      ctx.fillStyle = room.primary;
      ctx.font = '600 9px "IBM Plex Mono", monospace';
      const toolName = (a.name || 'tool').slice(0, 14);
      ctx.fillText(`> ${toolName}`, logX + 6, ly);

      if (a.detail) {
        ctx.fillStyle = '#6a7a8a';
        ctx.font = '400 9px "IBM Plex Mono", monospace';
        const maxW = logW - 16 - ctx.measureText(`> ${toolName} `).width;
        const detail = a.detail.slice(0, 50);
        ctx.fillText(detail, logX + 6 + ctx.measureText(`> ${toolName} `).width, ly, maxW);
      }
    } else if (a.type === 'text') {
      ctx.fillStyle = '#8899aa';
      ctx.font = '400 9px "IBM Plex Mono", monospace';
      const text = (a.content || '').replace(/\n/g, ' ').slice(0, 60);
      ctx.fillText(text, logX + 6, ly, logW - 12);
    } else {
      ctx.fillStyle = '#5a6a7a';
      ctx.font = '400 9px "IBM Plex Mono", monospace';
      const text = (a.content || a.detail || '').replace(/\n/g, ' ').slice(0, 60);
      ctx.fillText(text, logX + 6, ly, logW - 12);
    }
  }

  ctx.globalAlpha = 1;
  ctx.restore();
}

// --- Chat bar ---

export interface ChatBarParams {
  ctx: CanvasRenderingContext2D;
  canvasW: number;
  canvasH: number;
  rooms: RoomDef[];
  chatLog: { time: string; agent: string; text: string }[];
  chatMax: number;
}

export function drawChatBar(params: ChatBarParams) {
  const { ctx, canvasW, canvasH, rooms, chatLog, chatMax } = params;
  const chatBarH = 72;
  const y = canvasH - chatBarH;

  // Frosted bar
  ctx.fillStyle = 'rgba(2,8,16,0.85)';
  ctx.fillRect(0, y, canvasW, chatBarH);

  // Top edge glow
  const edgeGrad = ctx.createLinearGradient(0, y, canvasW, y);
  for (let i = 0; i < rooms.length; i++) {
    edgeGrad.addColorStop(i / rooms.length, rooms[i].primary + '22');
    edgeGrad.addColorStop((i + 0.5) / rooms.length, rooms[i].primary + '08');
  }
  ctx.fillStyle = edgeGrad;
  ctx.fillRect(0, y, canvasW, 1);

  ctx.font = '400 9px "IBM Plex Mono", monospace';
  ctx.textAlign = 'left';
  const visibleLog = chatLog.slice(-chatMax);
  for (let i = 0; i < visibleLog.length; i++) {
    const line = visibleLog[i];
    const ly = y + 14 + i * 13;
    const alpha = 0.3 + (i / visibleLog.length) * 0.7;
    ctx.globalAlpha = alpha;
    ctx.fillStyle = '#3a4a5a';
    ctx.fillText(`[${line.time}]`, 8, ly);
    ctx.fillStyle = '#00d4ff';
    ctx.fillText(line.agent, 56, ly);
    ctx.fillStyle = '#6a7a8a';
    ctx.fillText(': ' + line.text, 56 + ctx.measureText(line.agent).width, ly);
  }
  ctx.globalAlpha = 1;
}
