import type { Cell } from '@core/database-sql/generated/types';
import {
  type ReferenceNames,
  resultCell,
  resultCellText,
  unknownNames,
} from './answer-cell';
import { formatQueryValue, type QueryAnswer } from './query';

export const QUERY_CHART_MODES = [
  'bar',
  'line',
  'area',
  'scatter',
  'pie',
] as const;
export type QueryChartMode = (typeof QUERY_CHART_MODES)[number];
export type QueryDisplayMode = 'scalar' | 'table' | QueryChartMode;

/**
 * Saved chart settings: column aliases and two plain choices, never
 * evaluated expressions, copied result data, or renderer options.
 */
export type QueryChartConfig = {
  x: string;
  y: string[];
  title?: string;
  /** A column whose values split the one `y` series into groups. */
  color?: string;
  /** Stack bar or area series instead of grouping or overlapping them. */
  stack?: boolean;
};

/**
 * The chart to draw, independent of any charting library. It is derived
 * from a saved answer's display mode and chart settings, never stored.
 */
export type QueryChartSpec = {
  mark: QueryChartMode;
  x: string;
  y: string[];
  title?: string;
  color?: string;
  stack: boolean;
};

/** Most series one chart colors apart, the size of the chart palette. */
export const MAX_CHART_SERIES = 9;
const MAX_Y_COLUMNS = 5;
const MAX_POINTS = 300;
const MAX_PIE_CATEGORIES = 20;

export type QueryChartPoint = {
  /** A category label, a number, or a moment, as the x scale reads it. */
  x: string | number | Date;
  /** The x value as the database grid prints it. */
  label: string;
  series: string;
  /** Missing values are gaps, never zeroes. */
  value: number | null;
  tip: string;
};

export type QueryChartData = {
  config: QueryChartConfig;
  spec: QueryChartSpec;
  title: string;
  /** Categories keep row order; numbers and dates keep true distances. */
  scale: 'category' | 'number' | 'date';
  /** Distinct x labels in row order. */
  categories: string[];
  series: string[];
  points: QueryChartPoint[];
  /** Rows left off a number or time axis because their x is empty. */
  omitted: number;
};

export function isChartMode(mode: string | undefined): mode is QueryChartMode {
  return QUERY_CHART_MODES.some((chartMode) => chartMode === mode);
}

export function chartModeLabel(mode: QueryChartMode): string {
  return `${mode[0].toUpperCase()}${mode.slice(1)} chart`;
}

const nonEmptyName = (name: unknown): name is string =>
  typeof name === 'string' && !!name.trim();

export function parseQueryChart(value: unknown): QueryChartConfig | undefined {
  if (!value || typeof value !== 'object') return;
  const chart = value as Record<string, unknown>;
  const color = chart.color ?? undefined;
  const stack = chart.stack ?? undefined;
  if (
    !nonEmptyName(chart.x) ||
    !Array.isArray(chart.y) ||
    !chart.y.length ||
    chart.y.length > MAX_Y_COLUMNS ||
    !chart.y.every(nonEmptyName) ||
    new Set(chart.y).size !== chart.y.length ||
    chart.y.includes(chart.x) ||
    (chart.title !== undefined && typeof chart.title !== 'string') ||
    (color !== undefined &&
      (!nonEmptyName(color) ||
        color === chart.x ||
        chart.y.length !== 1 ||
        chart.y.includes(color))) ||
    (stack !== undefined && typeof stack !== 'boolean')
  )
    return;
  return {
    x: chart.x,
    y: [...chart.y],
    ...(chart.title ? { title: chart.title as string } : {}),
    ...(color ? { color } : {}),
    ...(stack ? { stack: true } : {}),
  };
}

/** What a saved chart draws; answers saved before `color` and `stack` read the same. */
export function queryChartSpec(
  mode: QueryChartMode,
  config: QueryChartConfig
): QueryChartSpec {
  return {
    mark: mode,
    x: config.x,
    y: [...config.y],
    ...(config.title ? { title: config.title } : {}),
    ...(config.color ? { color: config.color } : {}),
    stack: (mode === 'bar' || mode === 'area') && !!config.stack,
  };
}

const columnLabel = (name: string) => name.replaceAll('_', ' ');

/** A number cell's value; an empty cell is a gap; anything else is not a number. */
function numeric(cell: Cell | null | undefined): number | null | undefined {
  if (!cell) return null;
  return cell.type === 'number' ? cell.value : undefined;
}

type ChartResult =
  | { data: QueryChartData; error?: never }
  | { data?: never; error: string };

/** Validate the actual answer before drawing: missing values are gaps, never zeroes. */
export function prepareQueryChart(
  answer: QueryAnswer,
  mode: QueryChartMode,
  requested?: QueryChartConfig,
  references: ReferenceNames = unknownNames
): ChartResult {
  if (answer.columns.length < 2)
    return {
      error: 'A chart needs a label column and a number column.',
    };
  if (!answer.rows.length)
    return { error: 'There are no matching records to chart.' };
  const names = answer.columns.map((column) => column.name);
  const inferred = names.slice(1).filter((_, index) => {
    const values = answer.rows.map((row) => numeric(row[index + 1]));
    return (
      values.some((value) => typeof value === 'number') &&
      values.every((value) => value !== undefined)
    );
  });
  const config = requested ?? {
    x: names[0],
    y: inferred.slice(0, mode === 'pie' ? 1 : MAX_Y_COLUMNS),
  };
  const columnIndex = (name: string) => names.indexOf(name);
  if (
    !parseQueryChart(config) ||
    [config.x, ...config.y, ...(config.color ? [config.color] : [])].some(
      (name) => names.filter((candidate) => candidate === name).length !== 1
    )
  )
    return {
      error:
        'This chart’s columns are unavailable. Update the question or view the result table.',
    };
  if (answer.rows.length > (mode === 'pie' ? MAX_PIE_CATEGORIES : MAX_POINTS))
    return {
      error:
        mode === 'pie'
          ? 'This result has more than 20 categories. Choose Bar or ask for fewer groups.'
          : 'This result has more than 300 points. Ask for a summary or a smaller date range.',
    };
  const values = config.y.map((name) =>
    answer.rows.map((row) => numeric(row[columnIndex(name)]))
  );
  if (values.some((column) => column.some((value) => value === undefined)))
    return {
      error:
        'A chart needs numeric values. View the table or ask for a numeric summary.',
    };
  if (
    !values.some((column) => column.some((value) => typeof value === 'number'))
  )
    return { error: 'There are no numeric values to chart.' };
  const spec = queryChartSpec(mode, config);
  if (mode === 'pie') {
    const shares = values[0];
    if (
      config.y.length !== 1 ||
      config.color ||
      shares.some((value) => value !== null && value !== undefined && value < 0)
    )
      return {
        error:
          'A pie chart needs one series of nonnegative values. Choose Bar to compare these values.',
      };
    if (
      !shares.some(
        (value) => value !== null && value !== undefined && value > 0
      )
    )
      return {
        error: 'A pie chart needs at least one value greater than zero.',
      };
  }

  const cellAt = (row: QueryAnswer['rows'][number], name: string) =>
    resultCell(
      row[columnIndex(name)] ?? null,
      answer.columns[columnIndex(name)]
    );
  const text = (row: QueryAnswer['rows'][number], name: string) => {
    const cell = cellAt(row, name);
    return cell.kind === 'empty' ? 'Empty' : resultCellText(cell, references);
  };
  const xCells = answer.rows.map((row) => cellAt(row, config.x));
  const present = xCells.filter((cell) => cell.kind !== 'empty');
  const scale: QueryChartData['scale'] =
    present.length === 0
      ? 'category'
      : present.every((cell) => cell.kind === 'number')
        ? 'number'
        : present.every((cell) => cell.kind === 'date')
          ? 'date'
          : 'category';
  // Only a continuous axis has nowhere to put an empty x; bands label it.
  const continuous = scale !== 'category' && mode !== 'bar' && mode !== 'pie';
  const xOf = (cell: (typeof xCells)[number], label: string) => {
    if (!continuous) return label;
    if (cell.kind === 'number') return cell.value;
    if (cell.kind === 'date') return cell.date;
    return label;
  };

  const points: QueryChartPoint[] = [];
  let omitted = 0;
  answer.rows.forEach((row, rowIndex) => {
    const x = xCells[rowIndex];
    if (continuous && x.kind === 'empty') {
      omitted += 1;
      return;
    }
    const label = text(row, config.x);
    config.y.forEach((name, seriesIndex) => {
      const series = config.color ? text(row, config.color) : columnLabel(name);
      const value = values[seriesIndex][rowIndex] ?? null;
      points.push({
        x: xOf(x, label),
        label,
        series,
        value,
        tip: `${label}\n${series}: ${formatQueryValue(value)}`,
      });
    });
  });
  const series = [...new Set(points.map((point) => point.series))];
  if (series.length > MAX_CHART_SERIES)
    return {
      error: `This chart splits into more than ${MAX_CHART_SERIES} groups. Ask for fewer groups or view the table.`,
    };
  const drawn = mode === 'pie' ? foldPie(points) : points;
  return {
    data: {
      config,
      spec,
      title:
        config.title ||
        `${config.y.map(columnLabel).join(', ')} by ${columnLabel(config.x)}${
          config.color ? ` and ${columnLabel(config.color)}` : ''
        }`,
      scale,
      categories: [...new Set(drawn.map((point) => point.label))],
      series,
      points: drawn,
      omitted,
    },
  };
}

/**
 * A pie has one color per slice, so slices past the palette fold into
 * "Other", smallest first; the rest keep row order.
 */
function foldPie(points: QueryChartPoint[]): QueryChartPoint[] {
  if (points.length <= MAX_CHART_SERIES) return points;
  const kept = new Set(
    [...points]
      .sort((left, right) => (right.value ?? 0) - (left.value ?? 0))
      .slice(0, MAX_CHART_SERIES - 1)
  );
  const rest = points.filter((point) => !kept.has(point));
  const value = rest.reduce((sum, point) => sum + (point.value ?? 0), 0);
  const series = points[0].series;
  return [
    ...points.filter((point) => kept.has(point)),
    {
      x: 'Other',
      label: 'Other',
      series,
      value,
      tip: `Other (${rest.length} categories)\n${series}: ${formatQueryValue(value)}`,
    },
  ];
}
