export interface ChartPoint {
  timeMs: number;
  value: number | null;
}

export interface ChartSeries {
  label: string;
  color?: string;
  points: ChartPoint[];
  format?: (value: number) => string;
}

export type ChartScale = 'automatic' | 'percentage';

export interface MissingInterval {
  startMs: number;
  endMs: number;
  kind: 'gap' | 'stale';
}
