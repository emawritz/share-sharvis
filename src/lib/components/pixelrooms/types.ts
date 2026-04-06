// Shared types for PixelRooms component

export interface RoomDef {
  id: string;
  name: string;
  primary: string;
  glow: string;
  core: string;
}

export interface AgentState {
  machineId: string;
  state: 'idle' | 'working' | 'error' | 'completing';
  taskPrompt: string;
  elapsed: number;
  stateTimer: number;
}

export interface Bubble {
  machineId: string;
  text: string;
  color: string;
  timer: number;
  maxTimer: number;
}

export interface Particle {
  x: number;
  y: number;
  vx: number;
  vy: number;
  color: string;
  life: number;
  maxLife: number;
  size: number;
}

export interface ChatLine {
  time: string;
  agent: string;
  text: string;
  age: number;
}

export interface Mote {
  x: number;
  y: number;
  vx: number;
  vy: number;
  r: number;
  color: string;
  alpha: number;
  decay: number;
}

export interface ActivityItem {
  type: string;
  name?: string;
  detail?: string;
  content?: string;
}
