import { JSX, useState } from 'react';
import { Row } from 'react-bootstrap';
import { VscJson } from 'react-icons/vsc';
import { JSONTree } from 'react-json-tree';
import styled from 'styled-components';

// project imports
import { EntityDetailsConfig } from './configs';
import { DetailsMetadataProps } from '../EntityDetails';
import InfoValue from '@entities/shared/InfoValue';
import InfoHeader from '@components/entities/shared/InfoHeader';
import CodeEditor from '@components/shared/inputs/code/CodeEditor/CodeEditor';
import { getEntity } from '@thorpi/entities';
import { Entities } from '@models/entities';
import { BlankJsonEntity, JsonEntity } from '@models/entities/json';
import { FormatType } from '@utilities/rules/types';

const DocumentPreview = styled.div`
  background-color: var(--thorium-secondary-panel-bg);
  border: 1px solid var(--thorium-panel-border);
  border-radius: 0.375rem;
  padding: 0.75rem;
  font-size: 0.8rem;
  overflow: auto;
`;

const ParseError = styled.small`
  color: var(--thorium-danger, red);
`;

/// The theme to render this entities json document with
const JSON_TREE_THEME = {
  base00: 'transparent',
};

const JsonMetaInfo = ({ entity, pendingEntity, handleUpdate, editing }: DetailsMetadataProps<Entities.Json>): JSX.Element => {
  // track the raw text separately so a partially typed document isn't thrown away
  const [text, setText] = useState(() => JSON.stringify(pendingEntity.metadata.Json.data, null, 2));
  // track whether the text currently in the editor is parseable
  const [parseError, setParseError] = useState<string | null>(null);
  // remember the last edit mode we rendered so we can tell when it flips
  const [wasEditing, setWasEditing] = useState(editing);

  // re-seed the editor every time we enter edit mode so a cancelled edit is really discarded
  if (editing !== wasEditing) {
    setWasEditing(editing);
    if (editing) {
      setText(JSON.stringify(pendingEntity.metadata.Json.data, null, 2));
      setParseError(null);
    }
  }

  // handle any edits to this entities json document
  const handleTextChange = (updated: string) => {
    // always keep the editor showing exactly what the user typed
    setText(updated);
    try {
      // only push the document up to the pending entity when it parses
      const parsed: unknown = JSON.parse(updated);
      handleUpdate('metadata', { Json: { data: parsed } });
      setParseError(null);
    } catch (error) {
      // surface the parse failure so the user knows this edit won't be saved
      setParseError(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <Row className="mt-3">
      <InfoHeader>Document</InfoHeader>
      <InfoValue>
        {editing ? (
          <>
            <CodeEditor value={text} onChange={handleTextChange} format={FormatType.JSON} height="400px" />
            {parseError && <ParseError>{parseError}</ParseError>}
          </>
        ) : (
          <DocumentPreview>
            <JSONTree
              data={entity.metadata.Json.data}
              theme={JSON_TREE_THEME}
              invertTheme={false}
              hideRoot
              shouldExpandNodeInitially={() => true}
            />
          </DocumentPreview>
        )}
      </InfoValue>
    </Row>
  );
};

// Get Json entity details from the API
const getJsonDetails = (entityID: string, setError: (err: string) => void, updateEntity: (entity: JsonEntity) => void) => {
  void getEntity(entityID, setError).then((data) => {
    // check data is not null and is of Json kind
    if (data && data.kind == Entities.Json) {
      updateEntity(data);
    }
  });
};

const JsonDetailsConfig: EntityDetailsConfig<Entities.Json> = {
  getEntityDetails: getJsonDetails,
  EntityMetaInfo: JsonMetaInfo,
  BlankEntity: BlankJsonEntity,
  icon: (size: number) => <VscJson size={size} />,
};

export default JsonDetailsConfig;
