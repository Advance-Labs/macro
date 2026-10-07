import { z } from 'zod';
import type { RoutineTarget } from './routine-target';
import {
  type RoutineTriggerDraft,
  routineTriggerSchema,
} from './routine-triggers';

export type ScheduleFrequency = 'week' | 'month' | 'once';

export type ScheduleDraft = {
  id?: string;
  triggers?: RoutineTriggerDraft[];
  enabled?: boolean;
  name: string;
  prompt: string;
  frequency: ScheduleFrequency;
  time: string;
  /** Day-of-week values using the cron crate's 1-7 numbering (1=Sun). */
  daysOfWeek: string[];
  /** 1-31 day-of-month when frequency === "month". */
  dayOfMonth: string;
  target: RoutineTarget;
  onceAt?: string;
  timezone?: string;
};

/** Fields a caller can prefill when opening the routine composer. */
export type RoutineSeed = Pick<ScheduleDraft, 'name' | 'prompt' | 'triggers'>;

const routineSeedSchema = z.object({
  name: z.string(),
  prompt: z.string(),
  triggers: z.array(routineTriggerSchema).optional(),
});

/** Accept a seed from untyped split params only when it has the full shape. */
export function parseRoutineSeed(value: unknown): RoutineSeed | undefined {
  const parsed = routineSeedSchema.safeParse(value);
  return parsed.success ? parsed.data : undefined;
}
