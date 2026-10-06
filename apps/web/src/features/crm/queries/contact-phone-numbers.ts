import { throwOnErr } from '@core/util/result';
import { useMutation, useQuery } from '@tanstack/solid-query';
import type { Accessor } from 'solid-js';
import type { CrmRecordDependencies } from './dependencies';
import { crmKeys } from './keys';

const PHONE_NUMBERS_STALE_TIME = 60 * 1000;

/** A contact's phone numbers, in E.164, in the order they were entered. */
export function useContactPhoneNumbersQuery(
  deps: CrmRecordDependencies,
  contactId: Accessor<string>,
  enabled: Accessor<boolean>
) {
  return useQuery(
    () => {
      const id = contactId();
      return {
        queryKey: crmKeys.contactPhoneNumbers(id).queryKey,
        queryFn: async ({ signal }: { signal: AbortSignal }) => {
          const response = await throwOnErr(() =>
            deps.storage.getContactPhoneNumbers({ contactId: id, signal })
          );
          return response.phoneNumbers;
        },
        staleTime: PHONE_NUMBERS_STALE_TIME,
        enabled: enabled() && !!id,
      };
    },
    () => deps.client
  );
}

/**
 * Replace a contact's phone numbers with the list as typed. The server parses
 * and normalizes them; the stored list replaces the cache.
 */
export function useSetContactPhoneNumbersMutation(deps: CrmRecordDependencies) {
  return useMutation(
    () => ({
      mutationFn: async ({
        contactId,
        phoneNumbers,
      }: {
        contactId: string;
        phoneNumbers: string[];
      }) => {
        const response = await throwOnErr(() =>
          deps.storage.setContactPhoneNumbers({ contactId, phoneNumbers })
        );
        return response.phoneNumbers;
      },
      onSuccess: (phoneNumbers: string[], { contactId }) => {
        deps.client.setQueryData(
          crmKeys.contactPhoneNumbers(contactId).queryKey,
          phoneNumbers
        );
      },
    }),
    () => deps.client
  );
}
