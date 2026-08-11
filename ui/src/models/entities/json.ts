import { CreateEntity, Entities, Entity } from './entities';

/// A single line/document of arbitrary json
export type JsonMetaFields = {
  /// The parsed json document for this entity
  data: unknown;
};

export type JsonCreateMetaFields = JsonMetaFields;

export type JsonMeta = {
  Json: JsonMetaFields;
};

export type JsonCreateMeta = {
  Json: JsonCreateMetaFields;
};

export type JsonEntity = Entity<Entities.Json>;

export type CreateJsonEntity = CreateEntity<Entities.Json>;

export const BlankJsonEntity: JsonEntity = {
  id: '',
  name: '',
  groups: [],
  description: null,
  kind: Entities.Json,
  metadata: {
    Json: {
      data: {},
    },
  },
  tags: {},
  submitter: '',
  created: '',
  image: null,
};

export const BlankCreateJsonEntity: CreateJsonEntity = {
  name: '',
  groups: [],
  tags: {},
  description: null,
  kind: Entities.Json,
  metadata: {
    Json: {
      data: {},
    },
  },
};
