import { JSX, useState } from 'react';
import { Row } from 'react-bootstrap';
import styled from 'styled-components';

// project imports
import { EntityCreateConfig } from './config';
import { CreateMetadataProps } from '../EntityCreate';
import InfoHeader from '@entities/shared/InfoHeader';
import InfoValue from '@entities/shared/InfoValue';
import CodeEditor from '@components/shared/inputs/code/CodeEditor/CodeEditor';
import { Entities } from '@models/entities/entities';
import { BlankCreateJsonEntity } from '@models/entities/json';
import { FormatType } from '@utilities/rules/types';

const ParseError = styled.small`
  color: var(--thorium-danger, red);
`;

const JsonMetaInfo = ({ entity, onChange }: CreateMetadataProps<Entities.Json>): JSX.Element => {
  // track the raw text separately so a partially typed document isn't thrown away
  const [text, setText] = useState(() => JSON.stringify(entity.metadata.Json.data, null, 2));
  // track whether the text currently in the editor is parseable
  const [parseError, setParseError] = useState<string | null>(null);

  // handle any edits to this entities json document
  const handleTextChange = (updated: string) => {
    // always keep the editor showing exactly what the user typed
    setText(updated);
    try {
      // only push the document up to the pending entity when it parses
      const parsed: unknown = JSON.parse(updated);
      onChange('metadata', { Json: { data: parsed } });
      setParseError(null);
    } catch (error) {
      // surface the parse failure so the user knows this document can't be saved yet
      setParseError(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <Row>
      <InfoHeader>Document</InfoHeader>
      <InfoValue>
        <CodeEditor value={text} onChange={handleTextChange} format={FormatType.JSON} height="400px" />
        {parseError && <ParseError>{parseError}</ParseError>}
      </InfoValue>
    </Row>
  );
};

const JsonCreateConfig: EntityCreateConfig<Entities.Json> = {
  kind: Entities.Json,
  EntityMetadata: JsonMetaInfo,
  BlankCreateEntity: BlankCreateJsonEntity,
};

export default JsonCreateConfig;
