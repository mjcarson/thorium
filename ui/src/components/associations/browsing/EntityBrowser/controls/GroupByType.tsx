import { FaLayerGroup } from 'react-icons/fa6';

// project imports
import { useEntityBrowser } from '../EntityBrowserContext';
import { ToggleChip } from '../EntityBrowser.styled';

/**
 * Standalone group controls for the entity browser, reading state from {@link useEntityBrowser}:
 * an on-by-default "Group by Type" toggle that groups each level by node kind
 * under {@link LayerHeader}s (off renders one flat, sorted list). Rendered in the browser's own header row
 * (`BrowserHeader`) so it sits directly above the list, shared by the file-details tab and the dashboard.
 */
const GroupByType: React.FC = () => {
  const { groupByResource, setGroupByResource } = useEntityBrowser();
  return (
    <ToggleChip
      type="button"
      $active={groupByResource}
      $tone="accent"
      data-testid="entity-group-by-resource"
      aria-pressed={groupByResource}
      onClick={() => setGroupByResource(!groupByResource)}
    >
      <FaLayerGroup size={12} /> Group by Type
    </ToggleChip>
  );
};

export default GroupByType;
