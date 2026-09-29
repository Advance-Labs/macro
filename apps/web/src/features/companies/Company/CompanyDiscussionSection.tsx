import { useUserId } from '@core/context/user';
import { EntityDiscussion } from '@core/messages/EntityDiscussion';

/** Discussion on a CRM company, on the shared message components. */
export function CompanyDiscussionSection(props: {
  companyId: string;
  targetId?: string;
  navigationRequest?: number;
  buildLink?: (messageId: string) => string;
}) {
  const userId = useUserId();
  return (
    <EntityDiscussion
      parent={{ type: 'crm_company', id: props.companyId }}
      canWrite={!!userId()}
      link={{ type: 'company', id: props.companyId }}
      targetId={props.targetId}
      navigationRequest={props.navigationRequest}
      buildLink={props.buildLink}
    />
  );
}
