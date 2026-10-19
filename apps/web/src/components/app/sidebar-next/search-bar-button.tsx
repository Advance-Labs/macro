import { requestSearchFocus } from '@app/features/next-soup/soup-view/search-controllers';
import { useAnalytics } from '@app/lib/analytics/analytics-context';
import { globalSplitManager } from '@app/signal/splitLayout';
import { navigateToSidebarView } from '@components/app/app-sidebar/sidebar';
import { useSplitLayout } from '@components/app/split-layout/layout';
import { TOKENS } from '@core/hotkey/tokens';
import { cn } from '@ui';
import { SidebarSearchMenu } from '../app-sidebar/sidebar-search-menu';
import { RailLabel, useRailButton } from './rail-label';

/** Search and command-menu discovery in the compact rail. */
export const SearchRailButton = () => {
  const analytics = useAnalytics();
  const layout = useSplitLayout();
  const rail = useRailButton();

  const openSearch = (newSplit: boolean) => {
    analytics.track('sidebar_click', { view: 'search' });

    let split = globalSplitManager()?.activeSplit();
    const content = split?.content();

    if (
      !newSplit &&
      split &&
      content?.type === 'component' &&
      content.id === 'search'
    ) {
      requestSearchFocus(split.id);
      globalSplitManager()?.returnFocus();
      return;
    }

    split = navigateToSidebarView({
      viewId: 'search',
      shiftKey: newSplit,
      openWithSplit: layout.openWithSplit,
      referredFrom: 'sidebar',
    });
    if (split) requestSearchFocus(split.id);
    globalSplitManager()?.returnFocus();
  };

  return (
    <SidebarSearchMenu
      onSearch={openSearch}
      triggerLabel={<RailLabel>Search</RailLabel>}
      triggerProps={{
        size: rail.size(),
        ...rail.buttonProps(),
        variant: 'ghost',
        tooltipPlacement: 'right',
        hotkey: TOKENS.sidebar.goTo.search,
        'data-sidebar-next-search': '',
        class: cn(rail.class(), 'rounded-xl [&_svg]:size-5'),
      }}
    />
  );
};
