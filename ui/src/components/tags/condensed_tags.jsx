import React, { Fragment } from 'react';
import { Col, Row } from 'react-bootstrap';

// project imports
import { FormattedFileInfoTagKeys, TagBadge, filterIncludedTags, filterExcludedTags } from './tags';

const CondensedTags = ({ tags }) => {
  const excludeTags = [...FormattedFileInfoTagKeys, 'TLP', 'RESULTS', 'ATT&CK', 'MBC'];
  const generalTags = filterExcludedTags(tags, excludeTags);
  const fileInfoTags = filterIncludedTags(tags, FormattedFileInfoTagKeys);
  const tlpTags = filterIncludedTags(tags, ['TLP']);
  const attackTags = filterIncludedTags(tags, ['ATT&CK']);
  const mbcTags = filterIncludedTags(tags, ['MBC']);

  return (
    <Fragment>
      <hr />
      <Row>
        <Col className="d-flex justify-content-center wrap">
          {Object.keys(tlpTags).length > 0 &&
            Object.keys(tlpTags)
              .sort()
              .map((tagKey) =>
                Object.keys(tlpTags[tagKey])
                  .sort()
                  .map((tagValue) => <TagBadge key={'TLP_' + tagValue} tag={tagKey} value={tagValue} condensed={true} action={'link'} />),
              )}
          {Object.keys(generalTags)
            .sort()
            .map((tagKey) =>
              Object.keys(generalTags[tagKey])
                .sort()
                .map((tagValue) => <TagBadge key={'General_' + tagValue} tag={tagKey} value={tagValue} condensed={true} action={'link'} />),
            )}
        </Col>
      </Row>
      {Object.keys(fileInfoTags).length > 0 && (
        <Row>
          <Col className="d-flex justify-content-center wrap">
            {Object.keys(fileInfoTags)
              .sort()
              .map((tagKey) =>
                Object.keys(fileInfoTags[tagKey])
                  .sort()
                  .map((tagValue) => (
                    <TagBadge key={'FileInfo_' + tagValue} tag={tagKey} value={tagValue} condensed={true} action={'link'} />
                  )),
              )}
          </Col>
        </Row>
      )}
      {Object.keys(attackTags).length > 0 && (
        <Row>
          <Col className="d-flex justify-content-center wrap">
            {Object.keys(attackTags)
              .sort()
              .map((tagKey) =>
                Object.keys(attackTags[tagKey])
                  .sort()
                  .map((tagValue) => (
                    <TagBadge key={'Attack_' + tagValue} tag={'ATT&CK'} value={tagValue} condensed={true} action={'link'} />
                  )),
              )}
          </Col>
        </Row>
      )}
      {Object.keys(mbcTags).length > 0 && (
        <Row>
          <Col className="d-flex justify-content-center wrap">
            {Object.keys(mbcTags)
              .sort()
              .map((tagKey) =>
                Object.keys(mbcTags[tagKey])
                  .sort()
                  .map((tagValue) => <TagBadge key={'MBC_' + tagValue} tag={'MBC'} value={tagValue} condensed={true} action={'link'} />),
              )}
          </Col>
        </Row>
      )}
      {/* tags['Results'] &&
        <Row>
          <Col className='d-flex justify-content-center wrap'>
            {Object.keys(tags['Results']).sort().map((tagValue) => (
              <TagBadge
                key={'Results_' + tagValue}
                tag={'Results'}
                value={tagValue}
                condensed={true}
                action={'link'}/>
            ))}
          </Col>
        </Row>*/}
    </Fragment>
  );
};

export default CondensedTags;
