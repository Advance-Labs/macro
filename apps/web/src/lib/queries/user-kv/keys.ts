import { createQueryKeys } from '@lukemorales/query-key-factory';

export const userKvKeys = createQueryKeys('userKv', {
  namespace: (namespace: string) => [namespace],
});
