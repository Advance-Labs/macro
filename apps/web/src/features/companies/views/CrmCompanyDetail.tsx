import { ViewBreadcrumbs } from '@app/components/view-shell';
import { Contact } from '@app/features/contacts/Contact/Contact';
import { createSearchParams } from '@app/lib/split-router';
import { useGlobalBlockOrchestrator } from '@components/app/GlobalAppState';
import { SidePanel } from '@components/app/side-panel';
import { useSplitDisplayName } from '@components/app/split-layout/layoutUtils';
import { EntityIcon } from '@core/component/EntityIcon';
import { COMMENT_LINK_PARAM } from '@core/messages/comment-link';
import { createMethodRegistration } from '@core/orchestrator';
import SpinnerIcon from '@phosphor/spinner.svg';
import { type CompanyContact, useCompanyQuery } from '@queries/crm/companies';
import { useContactQuery } from '@queries/crm/contacts';
import { Button, Tooltip } from '@ui';
import {
  createComputed,
  createSignal,
  ErrorBoundary,
  type JSX,
  onMount,
  Show,
  Suspense,
} from 'solid-js';
import { Company } from '../Company/Company';
import { CrmCopyLinkButton } from '../components/CrmCopyLinkButton';
import {
  buildCrmCompanyUrl,
  buildCrmContactUrl,
  crmDetailSearch,
} from '../crm-route';

export function CrmCompanyDetail(props: {
  companyId?: string;
  contactId?: string;
  viewName: string;
  onClose: () => void;
  onOpenContact: (contact: CompanyContact) => void;
  onOpenCompany: (companyId: string) => void;
  navigation: JSX.Element;
}) {
  const orchestrator = useGlobalBlockOrchestrator();
  const [search, setSearch] = createSearchParams(crmDetailSearch);
  const [navigationRequest, setNavigationRequest] = createSignal(0);
  const companyQuery = useCompanyQuery(() => props.companyId ?? '');
  const contactQuery = useContactQuery(() => props.contactId ?? '');
  const companyName = () => companyQuery.company()?.name ?? 'Company';
  const contactName = () => {
    const contact = contactQuery.isSuccess ? contactQuery.data : undefined;
    return contact?.name ?? contact?.email ?? 'Contact';
  };
  useSplitDisplayName(() => (props.contactId ? contactName() : companyName()));
  const activeQuery = () =>
    props.contactId ? contactQuery : companyQuery.query;
  const closeDetail = () => {
    if (props.contactId && props.companyId) {
      props.onOpenCompany(props.companyId);
      return;
    }
    props.onClose();
  };
  const navigateToComment = async (params: Record<string, unknown>) => {
    const commentId = params[COMMENT_LINK_PARAM];
    if (typeof commentId !== 'string') return;
    setSearch({ commentId }, { history: 'replace' });
    setNavigationRequest((value) => value + 1);
  };
  createComputed(() => {
    const contactId = props.contactId;
    if (contactId) {
      const handle = orchestrator.registerBlockHandle('contact', contactId);
      createMethodRegistration(() => handle, {
        goToLocationFromParams: navigateToComment,
      });
      return;
    }
    const companyId = props.companyId;
    if (!companyId) return;
    const handle = orchestrator.registerBlockHandle('company', companyId);
    createMethodRegistration(() => handle, {
      goToLocationFromParams: navigateToComment,
    });
  });
  let container: HTMLDivElement | undefined;
  onMount(() => container?.focus());

  return (
    <ViewBreadcrumbs.Root
      value={
        props.contactId
          ? `contact:${props.contactId}`
          : `company:${props.companyId}`
      }
      onChange={(value) => {
        if (value === 'crm-view') props.onClose();
        if (value === `company:${props.companyId}` && props.companyId)
          props.onOpenCompany(props.companyId);
      }}
    >
      <ViewBreadcrumbs.Item
        value="crm-view"
        metadata={{ type: 'companies' }}
        order={0}
      >
        {(item) => (
          <Tooltip label={`Back to ${props.viewName}`} class="min-w-0">
            <ViewBreadcrumbs.Button
              isActive={item.isActive()}
              onClick={item.onSelect}
            >
              <span class="truncate">{props.viewName}</span>
            </ViewBreadcrumbs.Button>
          </Tooltip>
        )}
      </ViewBreadcrumbs.Item>
      <Show when={props.companyId}>
        {(companyId) => (
          <ViewBreadcrumbs.Item
            value={`company:${companyId()}`}
            metadata={{ type: 'company', id: companyId() }}
            order={1}
          >
            {(item) => (
              <Tooltip label={companyName()} class="min-w-0">
                <ViewBreadcrumbs.Button
                  isActive={item.isActive()}
                  onClick={item.onSelect}
                  class="gap-1.5"
                >
                  <EntityIcon
                    targetType="crm_company"
                    size="xs"
                    class="shrink-0"
                  />
                  <span class="truncate">{companyName()}</span>
                </ViewBreadcrumbs.Button>
              </Tooltip>
            )}
          </ViewBreadcrumbs.Item>
        )}
      </Show>
      <Show when={props.contactId}>
        {(contactId) => (
          <ViewBreadcrumbs.Item
            value={`contact:${contactId()}`}
            metadata={{ type: 'contact', id: contactId() }}
            order={props.companyId ? 2 : 1}
          >
            {(item) => (
              <Tooltip label={contactName()} class="min-w-0">
                <ViewBreadcrumbs.Button
                  isActive={item.isActive()}
                  onClick={item.onSelect}
                  class="gap-1.5"
                >
                  <EntityIcon targetType="contact" size="xs" class="shrink-0" />
                  <span class="truncate">{contactName()}</span>
                </ViewBreadcrumbs.Button>
              </Tooltip>
            )}
          </ViewBreadcrumbs.Item>
        )}
      </Show>
      <SidePanel.Root persistKey="crm-company">
        <div
          ref={container}
          tabindex={-1}
          class="flex size-full min-h-0 min-w-0 flex-col overflow-hidden outline-none"
        >
          <div class="flex h-12 min-w-0 shrink-0 items-center gap-3 border-b border-edge-muted px-4">
            {props.navigation}
            <ViewBreadcrumbs.Outlet
              aria-label="CRM record location"
              class="flex-1"
            />
            <div class="ml-auto flex shrink-0 items-center gap-2">
              <Show
                when={props.contactId}
                fallback={
                  <Show when={props.companyId}>
                    {(companyId) => (
                      <CrmCopyLinkButton type="company" id={companyId()} />
                    )}
                  </Show>
                }
              >
                {(contactId) => (
                  <CrmCopyLinkButton type="contact" id={contactId()} />
                )}
              </Show>
              <SidePanel.Toggle />
            </div>
          </div>
          <div class="relative min-h-0 min-w-0 flex-1">
            <Show when={props.contactId ?? props.companyId} keyed>
              {(recordId) => (
                <ErrorBoundary
                  fallback={(error, reset) => {
                    console.error('Failed to render CRM record', error);
                    return (
                      <DetailError
                        onRetry={reset}
                        onClose={closeDetail}
                        isContact={!!props.contactId}
                        backLabel={
                          props.contactId && props.companyId
                            ? 'Back to company'
                            : 'Back to view'
                        }
                      />
                    );
                  }}
                >
                  <Show
                    when={!activeQuery().isError}
                    fallback={
                      <DetailError
                        onRetry={() => void activeQuery().refetch()}
                        onClose={closeDetail}
                        isContact={!!props.contactId}
                        backLabel={
                          props.contactId && props.companyId
                            ? 'Back to company'
                            : 'Back to view'
                        }
                      />
                    }
                  >
                    <Suspense
                      fallback={
                        <div class="grid size-full place-items-center text-ink-muted">
                          <SpinnerIcon
                            aria-label={
                              props.contactId
                                ? 'Loading contact'
                                : 'Loading company'
                            }
                            class="size-5 animate-spin"
                          />
                        </div>
                      }
                    >
                      <Show
                        when={props.contactId}
                        fallback={
                          <Company
                            companyId={recordId}
                            headerToggle={false}
                            onHidden={props.onClose}
                            onOpenContact={props.onOpenContact}
                            discussionTargetId={search.commentId || undefined}
                            discussionNavigationRequest={navigationRequest()}
                            buildDiscussionLink={(messageId) =>
                              buildCrmCompanyUrl(recordId, messageId)
                            }
                          />
                        }
                      >
                        {(contactId) => (
                          <Contact
                            contactId={contactId()}
                            headerToggle={false}
                            onOpenCompany={(companyId) => {
                              props.onOpenCompany(companyId);
                              return true;
                            }}
                            discussionTargetId={search.commentId || undefined}
                            discussionNavigationRequest={navigationRequest()}
                            buildDiscussionLink={(messageId) =>
                              buildCrmContactUrl(contactId(), messageId)
                            }
                          />
                        )}
                      </Show>
                    </Suspense>
                  </Show>
                </ErrorBoundary>
              )}
            </Show>
          </div>
        </div>
      </SidePanel.Root>
    </ViewBreadcrumbs.Root>
  );
}

function DetailError(props: {
  onRetry: () => void;
  onClose: () => void;
  isContact: boolean;
  backLabel: string;
}) {
  return (
    <div class="flex size-full flex-col items-center justify-center gap-3 text-sm text-ink-muted">
      <p>This {props.isContact ? 'contact' : 'company'} couldn’t be loaded.</p>
      <div class="flex gap-2">
        <Button variant="outline" size="sm" onClick={props.onRetry}>
          Try again
        </Button>
        <Button variant="ghost" size="sm" onClick={props.onClose}>
          {props.backLabel}
        </Button>
      </div>
    </div>
  );
}
