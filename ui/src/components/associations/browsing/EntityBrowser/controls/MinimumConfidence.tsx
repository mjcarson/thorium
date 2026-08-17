import { useEntityBrowser } from '../EntityBrowserContext';
import { Confidence, ConfidenceEntries } from '@models/entities';
import { SelectChip, SortLabel, SortSelect } from './ControlsStyles';

const MinimumConfidence = () => {
  const { minConfidence, setMinConfidence } = useEntityBrowser();

  return (
    <SelectChip>
      <SortLabel htmlFor="minimum_confidence">Minimum Confidence:</SortLabel>
      <SortSelect id="minimum_confidence" value={minConfidence} onChange={(e) => setMinConfidence(e.target.value as Confidence)}>
        {ConfidenceEntries.map(([label, value]) => (
          <option key={value} value={value}>
            {label}
          </option>
        ))}
      </SortSelect>
    </SelectChip>
  );
};

export default MinimumConfidence;
