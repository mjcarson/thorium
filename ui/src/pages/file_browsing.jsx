import React, { useEffect, useState, Fragment } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { Helmet, HelmetProvider } from 'react-helmet-async';
import { Alert, Button, ButtonToolbar, Card, Col, Container, Form, Pagination, Row, Stack } from 'react-bootstrap';
import DatePicker from 'react-datepicker';
import 'react-datepicker/dist/react-datepicker.css';
import { FaFilter } from 'react-icons/fa';

// project imports
import { CondensedTags, LoadingSpinner, OverlayTipRight, Title, Subtitle, SelectGroups, SelectableDictionary } from '@components';
import { safeDateToStringConversion, safeStringToDateConversion, useAuth } from '@utilities';
import { listFiles } from '@thorpi';

// default number of results to render when listing files
const DEFAULT_LIST_LIMIT = 10;

// return a list of groups with no duplicates
const getUniqueGroupsList = (submissions) => {
  const uniqueGroupsList = [];
  for (const submission of submissions) {
    uniqueGroupsList.push(...submission.groups.filter((group) => !uniqueGroupsList.includes(group)));
  }
  return uniqueGroupsList;
};

// encode filters to search params
const encodeFiltersToParams = (filters) => {
  const encodedFilters = [];
  // encode limit
  encodedFilters.push(`limit=${encodeURIComponent(filters.limit)}`);
  // encode groups
  if (filters.hasOwnProperty('groups')) {
    if (Array.isArray(filters.groups)) {
      filters.groups.map((group) => {
        encodedFilters.push(`groups=${encodeURIComponent(group)}`);
      });
    } else {
      encodedFilters.push(`groups=${encodeURIComponent(filters.groups)}`);
    }
  }
  // encode nested tags
  for (const key in filters.tags) {
    if (filters.tags.hasOwnProperty(key)) {
      const values = filters.tags[key];
      values.forEach((value) => {
        encodedFilters.push(`tags[${encodeURIComponent(key)}]=${encodeURIComponent(value)}`);
      });
    }
  }
  if (filters.hasOwnProperty('start')) {
    encodedFilters.push(`start=${filters.start}`);
  }
  if (filters.hasOwnProperty('end')) {
    encodedFilters.push(`end=${filters.end}`);
  }
  // Join all parameters with '&' to form the query string
  return encodedFilters.join('&');
};

// decode search params to filters
const decodeParamsToFilters = (searchParams) => {
  const params = { "limit": DEFAULT_LIST_LIMIT };
  const tags = {};
  // Iterate over each search parameter
  for (const [key, value] of searchParams.entries()) {
    // skip empty values
    if (value == '') {
      continue;
    }
    // parse tags list
    if (key.startsWith('tags[')) {
      // break up tags keys from tags prefix
      const keyTokens = key.split(/\[|\]/).filter(Boolean);
      if (keyTokens.length == 2) {
        if (tags.hasOwnProperty(keyTokens[1])) {
          tags[keyTokens[1]].push(value);
        } else {
          tags[keyTokens[1]] = [value];
        }
      }
    // parse groups list for submission group membership
    } else if (key == "groups") {
      if (params.hasOwnProperty("groups")) {
        params.groups.push(value);
      } else {
        params["groups"] = [value];
      }
    // put all else keys as single key/value pairs in params
    } else {
      params[key] = value;
    }
  }
  if (Object.keys(tags).length > 0) {
    params['tags'] = tags;
  }
  return params;
};

// get files using filters and and an optional cursor
const getFiles = async (filters, cursor, reset) => {
  // reset cursor when filters have changed, caller must know this
  let requestCursor = cursor;
  if (reset) {
    requestCursor = null;
  }
  // get files list from API
  const res = await listFiles(
    filters,
    console.log,
    true, // details bool
    requestCursor,
  );
  return {
    filesList: res.data && Array.isArray(res.data) ? res.data : [],
    filesCursor: res.cursor ? res.cursor : false,
  };
};

// get all possible limit options including the current value
const getLimitOptions = (currentLimit) => {
  // add limit to limit options if it is not one of the defaults
  let limitOptions = [10, 25, 50, 100];
  if (currentLimit != 0 && !limitOptions.includes(currentLimit)) {
    limitOptions.push(parseInt(currentLimit));
    limitOptions = limitOptions.sort(function (a, b) {
      return a - b;
    });
  }
  return limitOptions;
}

const FileFilters = ({ setFilters, loading }) => {
  // show, apply and clear filters
  const [hideFilters, setHideFilters] = useState(true);
  // groups samples must be in, empty dict is all groups
  const [groups, setGroups] = useState({});
  // which tags to filter for, tags are additive
  const [tags, setTags] = useState([]);
  // date range for submitted files
  const [startDate, setStartDate] = useState(null);
  const [endDate, setEndDate] = useState(null);
  const maxDate = new Date();
  // url search params
  const [searchParams, setSearchParams] = useSearchParams();
  // number of files per page and build options array
  const [limit, setLimit] = useState(DEFAULT_LIST_LIMIT);
  const limitOptions = getLimitOptions(limit);
  const { userInfo } = useAuth();

  // get filters and user groups url params on initial page load
  // we do this after userInfo changes so we know a user's group membership
  useEffect(() => {
    readFilterParams();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [userInfo]);

  const updateBrowsingFilters = (limit) => {
    const filters = { limit: limit };
    // get a list selected group names
    const selectedGroups = Object.keys(groups).filter((group) => {
      return groups[group];
    });
    // don't add groups to list request when none selected
    if (selectedGroups.length != 0) {
      filters['groups'] = selectedGroups;
    }
    // must format dates for request if set, otherwise leave as null
    // toISOString converts the date to a UTC string (thorium uses UTC)
    if (startDate) {
      const safeStartDateString = safeDateToStringConversion(startDate);
      if (safeStartDateString) {
        filters['start'] = safeStartDateString;
      }
    }
    if (endDate) {
      const safeEndDateString = safeDateToStringConversion(endDate);
      if (safeEndDateString) {
        filters['end'] = safeEndDateString;
      }
    }
    // tag must have a key and value to be used in filter
    tags.forEach(function (tag) {
      const key = tag['key'];
      const value = tag['value'];
      if (key == '' || value == '') {
        return;
      }
      if (!filters.hasOwnProperty('tags')) {
        filters['tags'] = { [key]: [value] };
      } else if (!filters['tags'].hasOwnProperty(key)) {
        filters['tags'][key] = [value];
      } else {
        filters['tags'][key].push(value);
      }
    });
    // save only selected groups filters to filters object as array
    const savedGroups = [];
    Object.keys(groups).map((group) => {
      if (groups[group]) {
        savedGroups.push(group);
      }
    });
    filters['groups'] = savedGroups;
    // update url params w/filters
    setSearchParams(encodeFiltersToParams(filters));
    setFilters(filters);
  };

  // read filter values from url search query
  const readFilterParams = () => {
    // get filters from query params
    const filters = decodeParamsToFilters(searchParams);
    const requestFilters = structuredClone(filters);
    // generate default selected groups list with each group set to unselected/false
    const allGroups = {};
    if (userInfo && userInfo.groups) {
      userInfo.groups.map((group) => {
        allGroups[`${group}`] = false;
      });
    }
    // check if url parameters were passed in
    if (filters.hasOwnProperty('groups') && filters.groups.length > 0) {
      // only add groups that a user is a member of
      filters.groups.map((group) => {
        if (group && group != '') {
          // userInfo my not be set on initial page load
          // We assume groups in url are valid membership until userInfo is available
          if (userInfo && userInfo.groups && !userInfo.groups.includes(group)) {
            allGroups[group] = false;
          } else {
            allGroups[group] = true;
          }
        }
      });
    }
    setGroups(allGroups);
    // if set pull limit from url params
    if (filters.hasOwnProperty('limit') && !isNaN(filters.limit)) {
      setLimit(parseInt(filters.limit));
    }
    if (filters.hasOwnProperty('start')) {
      const safeStartDate = safeStringToDateConversion(filters.start);
      if (safeStartDate) {
        setStartDate(safeStartDate);
      }
    }
    if (filters.hasOwnProperty('end')) {
      const safeEndDate = safeStringToDateConversion(filters.end);
      if (safeEndDate) {
        setEndDate(safeEndDate);
      }
    }
    if (filters.hasOwnProperty('tags')) {
      const paramTags = [];
      // restructure keys to work with selectable dictionary
      for (const [key, values] of Object.entries(filters.tags)) {
        values.map((value) => {
          paramTags.push({ key: key, value: value });
        });
      }
      // only add filters
      if (paramTags.length > 0) {
        setTags(paramTags);
      }
    }
    setFilters(requestFilters);
  };

  // reset all filters and get updated list from API
  const resetFilters = () => {
    setTags([]);
    setStartDate(null);
    setEndDate(null);
    // reset each group value to false
    const allGroups = {};
    Object.keys(groups).map((group) => {
      allGroups[`${group}`] = false;
    });
    setGroups(allGroups);
    // reset url search query params
    const newFilters = {"limit": limit};
    setSearchParams(newFilters);
    setFilters(newFilters);
  };

  const submitFilterForm = (event) => {
    // apply filters when enter is clicked, otherwise ignore
    if (event.key === 'Enter') {
      updateBrowsingFilters(limit);
    }
  };

  return (
    <Fragment>
      <Row>
        <Col>
          <Row>
            <Col className="d-flex justify-content-center">
              <Title>Files</Title>
              <OverlayTipRight tip={`${hideFilters ? 'Expand' : 'Hide'} filters`}>
                <Button variant="" className="m-2 clear-btn" onClick={() => setHideFilters(!hideFilters)}>
                  <FaFilter size="18" />
                </Button>
              </OverlayTipRight>
            </Col>
          </Row>
          {!hideFilters && (
            <Card className="panel" onKeyDown={(event) => submitFilterForm(event)}>
              <Row>
                <Col className="d-flex justify-content-center mt-3">
                  <Subtitle>Groups</Subtitle>
                </Col>
              </Row>
              <Row className="mt-2">
                <Col className="d-flex justify-content-center groups-col">
                  <SelectGroups groups={groups} setGroups={setGroups} disabled={loading} />
                </Col>
              </Row>
              <Row className="my-2">
                <Col className="d-flex justify-content-center">
                  <Subtitle>Tags</Subtitle>
                </Col>
              </Row>
              <Row>
                <Col className="ms-5 me-1 pe-0">
                  <SelectableDictionary
                    disabled={loading}
                    entries={tags}
                    setEntries={setTags}
                    keys={null}
                    deleted={null}
                    setDeleted={void 0}
                    trim={true}
                    keyPlaceholder={'key'}
                    valuePlaceholder={'value'}
                  />
                </Col>
              </Row>
              <Row className="mt-3">
                <Col className="d-flex justify-content-end">
                  <Subtitle className="hide-element">Oldest Submission</Subtitle>
                  <Subtitle className="hide-small-element">Oldest</Subtitle>
                </Col>
                <Col className="d-flex justify-content-start">
                  <DatePicker
                    className="date-picker-input"
                    maxDate={startDate != null ? startDate : maxDate}
                    selected={endDate}
                    disabled={loading}
                    onChange={(date) => {
                      setEndDate(date);
                    }}
                  />
                </Col>
              </Row>
              <Row className="mt-1">
                <Col className="d-flex justify-content-end">
                  <Subtitle className="hide-element">Newest Submission</Subtitle>
                  <Subtitle className="hide-small-element">Newest</Subtitle>
                </Col>
                <Col className="d-flex justify-content-start">
                  <DatePicker
                    className="date-picker-input"
                    maxDate={maxDate}
                    minDate={endDate}
                    selected={startDate}
                    disabled={loading}
                    onChange={(date) => {
                      setStartDate(date);
                    }}
                  />
                </Col>
              </Row>
              <Row className="m-3">
                <Col className="d-flex justify-content-center">
                  <ButtonToolbar>
                    <Button
                      className="ok-btn"
                      disabled={loading}
                      onClick={() => {
                        updateBrowsingFilters(limit);
                      }}
                    >
                      Apply
                    </Button>
                    <Button
                      className="primary-btn"
                      disabled={loading}
                      onClick={() => {
                        resetFilters();
                      }}
                    >
                      Clear
                    </Button>
                  </ButtonToolbar>
                </Col>
              </Row>
              <Row className="mb-3 mt-1">
                <Col className="d-flex justify-content-center">
                  <Form.Select
                    className="m-0 limit-select"
                    value={limit}
                    disabled={loading}
                    onChange={(e) => {
                      setLimit(parseInt(e.target.value));
                      updateBrowsingFilters(e.target.value);
                    }}
                  >
                    {limitOptions.map((count) => (
                      <option key={count} value={count}>
                        {count}
                      </option>
                    ))}
                  </Form.Select>
                </Col>
              </Row>
            </Card>
          )}
        </Col>
      </Row>
    </Fragment>
  );
};

const FilesList = ({ filters, setLoading }) => {
  // filter related values
  const [files, setFiles] = useState([]);
  // paging/cursor values
  const [cursor, setCursor] = useState(null);
  const [page, setPage] = useState(0);
  const [maxPage, setMaxPage] = useState(1);
  // whether content is currently loading
  const [loadingFiles, setLoadingFiles] = useState(true);

  // Get a files list using set filters
  const getFilesPage = async (reset) => {
    setLoading(true);
    setLoadingFiles(true);
    // get more files and updated cursor
    const { filesList, filesCursor } = await getFiles(filters, cursor, reset);
    setCursor(filesCursor);
    // API responded, no longer waiting
    setLoading(false);
    setLoadingFiles(false);
    // save any listed files if request was successful
    let allFiles = [];
    if (reset) {
      allFiles = filesList;
    } else {
      allFiles = [...files, ...filesList];
    }
    setMaxPage(Math.ceil(allFiles.length / filters.limit));
    setFiles(allFiles);
  };

  // get files whenever filters changes
  useEffect(() => {
    // don't get files on initial page load
    if (filters != null) {
      getFilesPage(true);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filters]);

  // update the displayed page and retrieve new results when end of
  // already fetched results is reached
  const updatePage = (page) => {
    // Trigger getting more files when paging past end of already
    // retrieved files
    if (page == maxPage && !loadingFiles) {
      getFilesPage(false);
    }
    setPage(page);
  };

  // limit may not be set in the filter initially
  const limit = Object(filters).hasOwnProperty("limit") ? filters.limit: DEFAULT_LIST_LIMIT;
  return (
    <Fragment>
      <Row className="d-flex justify-content-center">
        <Card className="basic-card panel">
          <Card.Body>
            <Row>
              <Col className="d-flex justify-content-center sha256-col">SHA256</Col>
              <Col className="d-flex justify-content-center submissions-col">Submissions</Col>
              <Col className="d-flex justify-content-center groups-col hide-element">Group(s)</Col>
              <Col className="d-flex justify-content-center submitters-col">Submitter(s)</Col>
            </Row>
          </Card.Body>
        </Card>
      </Row>
      {!loadingFiles &&
        files.slice(page * limit, page * limit + limit).map((sample, idx) => (
          <Row key={`${sample.sha256}_${idx}`} className="d-flex justify-content-center">
            <Card className="basic-card panel">
              <Card.Body>
                <Link to={`/file/${sample.sha256}`} className="no-decoration">
                  <Row className="highlight-card">
                    <Col className="d-flex justify-content-center sha256-col sha256-hide">{sample.sha256}</Col>
                    <Col className="d-flex justify-content-center small-sha">{sample.sha256.substr(0, 30) + '...'}</Col>
                    <Col className="d-flex justify-content-center submissions-col">{sample.submissions.length}</Col>
                    <Col className="d-flex justify-content-center groups-col hide-element">
                      <small>
                        <i>
                          {getUniqueGroupsList(sample.submissions).toString().length > 75
                            ? getUniqueGroupsList(sample.submissions).toString().replaceAll(',', ', ').substring(0, 75) + '...'
                            : getUniqueGroupsList(sample.submissions).toString().replaceAll(',', ', ')}
                        </i>
                      </small>
                    </Col>
                    <Col className="d-flex justify-content-center submitters-col">
                      {sample.tags.submitter ? (
                        <small>
                          <i>
                            {Object.keys(sample.tags.submitter).toString().length > 75
                              ? Object.keys(sample.tags.submitter).toString().replaceAll(',', ', ').substring(0, 75) + '...'
                              : Object.keys(sample.tags.submitter).toString().replaceAll(',', ', ')}
                          </i>
                        </small>
                      ) : null}
                    </Col>
                  </Row>
                </Link>
                <Row>
                  {Object.keys(sample.tags).length > 1 || (Object.keys(sample.tags).length == 1 && !sample.tags.submitter) ? (
                    <CondensedTags tags={sample.tags} />
                  ) : null}
                </Row>
              </Card.Body>
            </Card>
          </Row>
        ))}
      <LoadingSpinner loading={loadingFiles}></LoadingSpinner>
      {files.length == 0 && !loadingFiles && (
        <Row>
          <Alert variant="info" className="d-flex justify-content-center m-1">
            No files found
          </Alert>
        </Row>
      )}
      {files.length > 0 && (
        <Row className="mt-3">
          <Col className="d-flex justify-content-center">
            <Pagination>
              <Pagination.Item onClick={() => updatePage(page - 1)} disabled={page == 0}>
                Back
              </Pagination.Item>
              <Pagination.Item onClick={() => updatePage(page + 1)} disabled={!cursor && page + 1 >= maxPage}>
                Next
              </Pagination.Item>
            </Pagination>
          </Col>
        </Row>
      )}
    </Fragment>
  );
};

const FilesBrowsingContainer = () => {
  const [loading, setLoading] = useState(false);
  const [filters, setFilters] = useState(null);
  return (
    <HelmetProvider>
      <Container>
        <Helmet>
          <title>Files &middot; Thorium</title>
        </Helmet>
        <Stack>
          <FileFilters setFilters={setFilters} loading={loading} />
          <FilesList filters={filters} setLoading={setLoading} loading={loading} />
        </Stack>
      </Container>
    </HelmetProvider>
  );
};

export default FilesBrowsingContainer;
