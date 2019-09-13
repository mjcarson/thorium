import React, { useEffect, useState, Fragment } from 'react';
import { useSearchParams } from 'react-router-dom';
import { Helmet, HelmetProvider } from 'react-helmet-async';
import { Alert, Button, ButtonToolbar, Card, Col, Container, Form, Pagination, Row } from 'react-bootstrap';
import DatePicker from 'react-datepicker';
import 'react-datepicker/dist/react-datepicker.css';
import { FaFilter } from 'react-icons/fa';

// project imports
import { OverlayTipRight, Title, Subtitle, SelectGroups, LoadingSpinner } from '@components';
import { safeDateToStringConversion, safeStringToDateConversion, useAuth } from '@utilities';
import { listRepos } from '@thorpi';

// default number of results to render when listing repos
const DEFAULT_LIST_LIMIT = 10;

const RepoItem = ({ repo }) => {
  // <Link to={`/repo/${repo.name}`} className='no-decoration m-0'></Link>
  return (
    <Card className="basic-card panel">
      <Card.Body>
        <Row className="highlight-card">
          <Col>{repo.name}</Col>
          <Col>{JSON.stringify(repo.submissions.length)}</Col>
          <Col>{JSON.stringify(repo.provider)}</Col>
        </Row>
      </Card.Body>
    </Card>
  );
};

const ReposList = () => {
  // show, apply and clear filters
  const [hideFilters, setHideFilters] = useState(true);
  // filter related values
  const [repos, setRepos] = useState([]);
  const [limit, setLimit] = useState(DEFAULT_LIST_LIMIT);
  const [groups, setGroups] = useState({});
  const [tagKey, setTagKey] = useState('');
  const [tagValue, setTagValue] = useState('');
  const [startDate, setStartDate] = useState(null);
  const [endDate, setEndDate] = useState(null);
  const maxDate = new Date();
  // paging/cursor values
  const [cursor, setCursor] = useState(null);
  const [page, setPage] = useState(0);
  const [maxPage, setMaxPage] = useState(1);
  const [hasMoreRepos, setHasMoreRepos] = useState(true);
  // whether content is currently loading
  const [loading, setLoading] = useState(false);
  const [updateFilters, setUpdateFilters] = useState(false);
  const { userInfo, checkCookie } = useAuth();
  const [searchParams, setSearchParams] = useSearchParams();

  // when filters change, get fresh list from API
  // need to do this in useEffect to ensure updates from setState
  useEffect(() => {
    if (updateFilters) {
      updateFilterParams();
      getFreshReposList();
      setUpdateFilters(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [updateFilters, limit]);

  // get filters and user groups url params on initial page load
  // we do this after userInfo changes so we know a user's group membership
  useEffect(() => {
    readFilterParams();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [userInfo]);

  // Get a repos list using set filters
  const getRepos = async (reset) => {
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
      filters['start'] = safeDateToStringConversion(startDate);
    }
    if (endDate) {
      filters['end'] = safeDateToStringConversion(endDate);
    }

    // tag must have a key and value to be used in filter
    if (tagKey != '' && tagValue != '') {
      filters['key'] = tagKey;
      filters['value'] = tagValue;
    }

    // reset cursor when filters have changed, caller must know this
    let requestCursor = cursor;
    if (reset) {
      requestCursor = null;
    }
    // get files list from API
    const res = await listRepos(
      filters,
      checkCookie,
      true, // details bool
      requestCursor,
    );

    // API responded, no longer waiting
    setLoading(false);
    // save any listed files if request was successful
    if (res && res.data) {
      let allRepos = [];
      if (reset) {
        allRepos = res.data;
      } else {
        allRepos = [...repos, ...res.data];
      }
      setMaxPage(Math.ceil(allRepos.length / limit));
      setRepos(allRepos);
      // save cursor or if no more files then set list complete boolean
      if (res.cursor) {
        setCursor(res.cursor);
      } else {
        // no cursor is returned when exausted
        setHasMoreRepos(false);
      }
    }
  };

  // save filters in url search query parameters
  const updateFilterParams = () => {
    // build url search query params object, limit is always set
    const newFilterParams = { limit: limit };
    // save as ISO date string in url params
    if (endDate) {
      const safeEndDateString = safeDateToStringConversion(endDate);
      if (safeEndDateString) {
        newFilterParams['end'] = safeEndDateString;
      }
    }
    if (startDate) {
      // save as ISO date string in url params
      const safeStartDateString = safeDateToStringConversion(startDate);
      if (safeStartDateString) {
        newFilterParams['start'] = safeStartDateString;
      }
    }
    if (tagKey && tagValue) {
      newFilterParams['key'] = tagKey;
      newFilterParams['value'] = tagValue;
    }
    // save only selected groups filters to filters object as array
    const savedGroups = [];
    Object.keys(groups).map((group) => {
      if (groups[group]) {
        savedGroups.push(group);
      }
    });
    newFilterParams['group'] = savedGroups;
    // update url params w/ filters
    setSearchParams(newFilterParams);
  };

  // read filter values from url search query
  const readFilterParams = () => {
    // get filters from query params
    const savedGroups = searchParams.getAll('group');
    const savedLimit = searchParams.get('limit');
    const savedStartDate = searchParams.get('start');
    const savedEndDate = searchParams.get('end');
    const savedTagKey = searchParams.get('key');
    const savedTagValue = searchParams.get('value');
    // generate default selected groups list with each group set to unselected/false
    const allGroups = {};
    if (userInfo && userInfo.groups) {
      userInfo.groups.map((group) => {
        allGroups[`${group}`] = false;
      });
    }
    // check if url parameters were passed in
    if (savedGroups.length > 0 || savedStartDate || savedEndDate || savedTagKey || savedTagValue || savedLimit) {
      if (savedGroups.length > 0) {
        // only add groups that a user is a member of
        savedGroups.map((group) => {
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
      // if set pull limit from url params
      if (savedLimit && !isNaN(savedLimit)) {
        setLimit(parseInt(savedLimit));
      }
      if (savedStartDate) {
        const safeStartDate = safeStringToDateConversion(savedStartDate);
        if (safeStartDate) {
          setStartDate(safeStartDate);
        }
      }
      if (savedEndDate) {
        const safeEndDate = safeStringToDateConversion(savedEndDate);
        if (safeEndDate) {
          setEndDate(safeEndDate);
        }
      }
      if (savedTagKey) {
        setTagKey(savedTagKey);
        setTagValue(savedTagValue);
      }
    }

    // groups always gets set query params or default false values
    setGroups(allGroups);
    setUpdateFilters(true);
  };

  // update the displayed page and retrieve new results when end of
  // already fetched results is reached
  const updatePage = (page) => {
    // Trigger getting more files when paging past end of already
    // retrieved files
    if (page == maxPage && !loading) {
      setLoading(true);
      getRepos(false);
    }
    setPage(page);
  };

  // reset all filters and get updated list from API
  const resetFilters = () => {
    setTagKey('');
    setTagValue('');
    setStartDate(null);
    setEndDate(null);
    // reset each group value to false
    const allGroups = {};
    Object.keys(groups).map((group) => {
      allGroups[`${group}`] = false;
    });
    setGroups(allGroups);
    // reset url search query params
    setSearchParams({});
    // set that filters have been updated which triggers an updated files listing API request
    setUpdateFilters(true);
  };

  // clear the existing list and request fresh list from API
  const getFreshReposList = () => {
    if (!loading) {
      setLoading(true);
      // reseting the list op requires resetting the page and has "more files" boolean
      setPage(0);
      setHasMoreRepos(true);
      getRepos(true);
    }
  };

  const submitFilterForm = (event) => {
    // apply filters when enter is clicked, otherwise ignore
    if (event.key === 'Enter') {
      setUpdateFilters(true);
    }
  };

  // add limit to limit options if it is not one of the defaults
  let limitOptions = [10, 25, 50, 100];
  if (limit != 0 && !limitOptions.includes(limit)) {
    limitOptions.push(parseInt(limit));
    limitOptions = limitOptions.sort(function (a, b) {
      return a - b;
    });
  }

  return (
    <>
      <Row>
        <Col>
          <Row>
            <Col className="d-flex justify-content-center">
              <Title>Repos</Title>
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
              <Row className="mt-3">
                <Col className="d-flex justify-content-end">
                  <Subtitle className="hide-element">Oldest Submission</Subtitle>
                  <Subtitle className="hide-small-element">Oldest</Subtitle>
                </Col>
                <Col className="d-flex justify-content-start">
                  <DatePicker
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
              <Row className="my-2">
                <Col className="d-flex justify-content-center">
                  <Subtitle>Tag</Subtitle>
                </Col>
              </Row>
              <Row>
                <Col className="ms-5 me-1 pe-0">
                  <Form.Control type="text" value={tagKey} placeholder="key" onChange={(e) => setTagKey(String(e.target.value))} />
                </Col>
                <Col className="me-5 ms-1 ps-0">
                  <Form.Control type="text" value={tagValue} placeholder="value" onChange={(e) => setTagValue(String(e.target.value))} />
                </Col>
              </Row>
              <Row className="m-3">
                <Col className="d-flex justify-content-center">
                  <ButtonToolbar>
                    <Button
                      className="ok-btn"
                      disabled={loading}
                      onClick={() => {
                        updateFilters(buildFilterMap());
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
                      setUpdateFilters(true);
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
      <Row className="d-flex justify-content-center">
        <Card className="basic-card panel">
          <Card.Body>
            <Row>
              <Col className="d-flex justify-content-center">Repo</Col>
              <Col className="d-flex justify-content-center">Submission(s)</Col>
              <Col className="d-flex justify-content-center">Provider(s)</Col>
            </Row>
          </Card.Body>
        </Card>
      </Row>
      {/* eslint-disable-next-line max-len*/}
      {!loading &&
        repos.slice(page * limit, page * limit + limit).map((repo, idx) => (
          <Row key={`${repo.sha256}_${idx}`} className="d-flex justify-content-center">
            <RepoItem repo={repo} />
          </Row>
        ))}
      <LoadingSpinner loading={loading}></LoadingSpinner>
      {repos.length == 0 && !loading && (
        <Row>
          <Alert variant="info" className="d-flex justify-content-center m-1">
            No files found
          </Alert>
        </Row>
      )}
      {repos.length > 0 && (
        <Row className="mt-3">
          <Col className="d-flex justify-content-center">
            <Pagination>
              <Pagination.Item onClick={() => updatePage(page - 1)} disabled={page == 0}>
                Back
              </Pagination.Item>
              <Pagination.Item onClick={() => updatePage(page + 1)} disabled={!hasMoreRepos && page + 1 >= maxPage}>
                Next
              </Pagination.Item>
            </Pagination>
          </Col>
        </Row>
      )}
    </>
  );
};

const RepoBrowsingContainer = () => {
  return (
    <HelmetProvider>
      <Container>
        <Helmet>
          <title>Repositories &middot; Thorium</title>
        </Helmet>
        <Row>
          <Col>
            <ReposList />
          </Col>
        </Row>
      </Container>
    </HelmetProvider>
  );
};

export default RepoBrowsingContainer;
