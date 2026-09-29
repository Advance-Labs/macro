import { useUserId } from '@core/context/user';
import { EntityDiscussion } from '@core/messages/EntityDiscussion';

/** Discussion on a CRM contact, on the shared message components. */
export function ContactDiscussionSection(props: {
  contactId: string;
  targetId?: string;
  navigationRequest?: number;
  buildLink?: (messageId: string) => string;
}) {
  const userId = useUserId();
  return (
    <EntityDiscussion
      parent={{ type: 'crm_contact', id: props.contactId }}
      canWrite={!!userId()}
      link={{ type: 'contact', id: props.contactId }}
      targetId={props.targetId}
      navigationRequest={props.navigationRequest}
      buildLink={props.buildLink}
    />
  );
}
