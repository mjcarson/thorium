import React from 'react';
import { Link } from 'react-router-dom';
import { Row } from 'react-bootstrap';

// project imports
import { EntityBrowseConfig } from './config';
import {
  BrowsingCard,
  BrowsingContents,
  EntityGroups,
  EntityName,
  EntitySecondary,
  EntitySubmitters,
  LinkFields,
} from '@entities/browsing/shared';
import CondensedEntityTags from '@components/tags/condensed/CondensedEntityTags';
import { listEntities } from '@thorpi/entities';
import { Filters } from '@models/search';
import { Entities } from '@models/entities/entities';
import { JsonEntity } from '@models/entities/json';
import { getDetailsBasePathByEntity } from '@components/entities/details/EntityDetailsRoutes';

interface JsonItemProps {
  entity: JsonEntity;
}

const JsonItem: React.FC<JsonItemProps> = ({ entity }) => {
  return (
    <BrowsingCard>
      <BrowsingContents>
        <Link to={`${getDetailsBasePathByEntity(Entities.Json)}/${entity.id}`} state={{ entity: entity }} className="no-decoration">
          <LinkFields>
            <EntityName>{entity.name}</EntityName>
            <EntitySecondary>{entity.created}</EntitySecondary>
            <EntityGroups>
              <small>
                <i>
                  {entity.groups &&
                    (entity.groups.toString().length > 75
                      ? entity.groups.toString().replaceAll(',', ', ').substring(0, 75) + '...'
                      : entity.groups.toString().replaceAll(',', ', '))}
                </i>
              </small>
            </EntityGroups>
            <EntitySubmitters>{entity.submitter}</EntitySubmitters>
          </LinkFields>
        </Link>
        {entity.tags != undefined && <hr />}
        {entity.tags && Object.keys(entity.tags).length > 1 ? <CondensedEntityTags resource={Entities.Json} tags={entity.tags} /> : null}
      </BrowsingContents>
    </BrowsingCard>
  );
};

const JsonListHeaders = () => (
  <BrowsingCard>
    <BrowsingContents>
      <Row>
        <EntityName>Name</EntityName>
        <EntitySecondary>Created</EntitySecondary>
        <EntityGroups>Group(s)</EntityGroups>
        <EntitySubmitters>Submitter</EntitySubmitters>
      </Row>
    </BrowsingContents>
  </BrowsingCard>
);

const getJsonEntities = async (filters: Filters, cursor: string | null, errorHandler: (error: string) => void) => {
  const listFilters = structuredClone(filters);
  listFilters.kinds = [Entities.Json];
  const { entityList, entityCursor } = await listEntities(listFilters, errorHandler, true, cursor);
  return { entitiesList: entityList as JsonEntity[], entitiesCursor: entityCursor };
};

const JsonBrowsingConfig: EntityBrowseConfig<Entities.Json> = {
  docTitle: 'Json · Thorium',
  title: 'Json',
  typeLabel: '',
  kind: Entities.Json,
  creatable: true,
  entityHeaders: <JsonListHeaders />,
  renderEntity: (entity) => <JsonItem entity={entity} />,
  fetchEntities: getJsonEntities,
};

export default JsonBrowsingConfig;
