/* eslint-disable react-hooks/exhaustive-deps */
import React, { useEffect, useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { Alert, Button, Card, Col, Form, Pagination, Row, Stack } from 'react-bootstrap';
import DOMPurify from 'dompurify';
import parse from 'html-react-parser';
import DatePicker from 'react-datepicker';
import 'react-datepicker/dist/react-datepicker.css';
import { FaFilter } from 'react-icons/fa';

// project imports
import { LoadingSpinner, OverlayTipRight, SelectGroups } from '@components';
import { safeDateToStringConversion, safeStringToDateConversion, useAuth } from '@utilities';
import { searchResults } from '@thorpi';

// get hash of a file from result ID
const getSha256 = (id) => {
  const splitID = id.split('-');
  if (splitID.length > 0) {
    return splitID[0];
  }
  return '';
};

// get group name from result ID
const getGroup = (id) => {
  const splitID = id.split('-');
  if (splitID.length > 1) {
    return splitID[1];
  }
  return '';
};

// replace kibana mark up tags w/ highlight html tag
const highlightResult = (result) => {
  const highlightStart = result.toString().replaceAll('@kibana-highlighted-field@', '<mark>');
  const highlightFinish = highlightStart.replaceAll('@/kibana-highlighted-field@', '</mark>');
  // we must santize the output that will be rendered as html
  const clean = DOMPurify.sanitize(highlightFinish, { ALLOWED_TAGS: ['mark'] });
  return parse(`${clean}`);
};

// component containing search bar and related functionality
const Search = () => {
  const [searching, setSearching] = useState(false);
  const [staleSearchResults, setStaleSearchResults] = useState(false);
  const { userInfo } = useAuth();
  // store search string value
  const [hideFilters, setHideFilters] = useState(true);
  const [results, setResults] = useState([]);
  const [hasQuery, setHasQuery] = useState(false);
  const [lastValidQuery, setLastValidQuery] = useState('');
  const [limit, setLimit] = useState(10);
  const [groups, setGroups] = useState({});
  const [startDate, setStartDate] = useState(null);
  const [endDate, setEndDate] = useState(null);
  const maxDate = new Date();
  // the id of the cursor for paging search results
  const [cursor, setCursor] = useState(null);
  const [page, setPage] = useState(0);
  const [maxPage, setMaxPage] = useState(1);
  const [searchError, setSearchError] = useState('');
  const [hasMoreSearchResults, setHasMoreSearchResults] = useState(true);
  const [searchParams, setSearchParams] = useSearchParams();
  const [query, setQuery] = useState('');

  // read filter values from url search query params
  const readURLSearchParams = () => {
    const savedQuery = searchParams.get('query');
    const savedGroups = searchParams.getAll('group');
    const savedLimit = searchParams.get('limit');
    const savedStartDate = searchParams.get('start');
    const savedEndDate = searchParams.get('end');

    // generate default selected groups list with each group set to unselected/false
    const allGroups = {};
    if (userInfo && userInfo.groups) {
      userInfo.groups.map((group) => {
        allGroups[`${group}`] = false;
      });
    }
    // check if url parameters were passed in
    if (savedGroups.length > 0 || savedStartDate || savedEndDate || savedQuery || savedLimit) {
      if (savedGroups) {
        savedGroups.map((group) => {
          if (group && group != '' && userInfo && userInfo.groups && userInfo.groups.includes(group)) {
            allGroups[group] = true;
          }
        });
      }
      // if limit is in the URL params then grab that
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
      if (savedQuery && savedQuery != '') {
        setQuery(savedQuery);
      }
    }

    // groups always gets set, from query params or default false values
    setGroups(allGroups);
  };

  // save filters in url search query parameters
  const updateURLSearchParams = () => {
    // build url search query params object, limit is always set
    const newSearchParams = { limit: limit };
    // add the query to the params
    if (query) {
      newSearchParams['query'] = query;
    }
    // save as ISO date string in url params
    if (endDate) {
      const safeEndDateString = safeDateToStringConversion(endDate);
      if (safeEndDateString) {
        newSearchParams['end'] = safeEndDateString;
      }
    }
    if (startDate) {
      // save as ISO date string in url params
      const safeStartDateString = safeDateToStringConversion(startDate);
      if (safeStartDateString) {
        newSearchParams['start'] = safeStartDateString;
      }
    }
    // save only selected groups filters to filters object as array
    const savedGroups = [];
    Object.keys(groups).map((group) => {
      if (groups[group]) {
        savedGroups.push(group);
      }
    });
    newSearchParams['group'] = savedGroups;
    // update url params w/ filters
    setSearchParams(newSearchParams);
  };

  // get user details
  const getResults = async (resetSearch) => {
    // get a list selected group names
    let selectedGroups = Object.keys(groups).filter((group) => {
      return groups[group];
    });
    // don't add groups to search request when none selected
    if (selectedGroups.length == 0) {
      selectedGroups = null;
    }

    // must format dates for request if set, otherwise leave as null
    let formattedStartDate = null;
    if (startDate) {
      formattedStartDate = safeDateToStringConversion(startDate);
    }
    let formattedEndDate = null;
    if (endDate) {
      formattedEndDate = safeDateToStringConversion(endDate);
    }

    // reset paging variables for search with new query/filters
    let requestCursor = cursor;
    let requestQuery = query;

    // reset the cursor and any errors when doing a new search
    if (resetSearch) {
      requestCursor = null;
      setSearchError('');
    }

    // query is not valid, this is hit when paging old results
    if (requestQuery != lastValidQuery && !resetSearch) {
      requestQuery = lastValidQuery;
      requestCursor = cursor;
    }

    // request new results when query is not blank
    if (query != '') {
      setSearching(true);

      // get results from API
      const reqResults = await searchResults(
        // remove leading and trailing whitespace, messes up the results returned
        requestQuery.trim(),
        setSearchError,
        selectedGroups,
        formattedStartDate,
        formattedEndDate,
        requestCursor,
        limit,
      );

      // save any returned results
      if (reqResults) {
        setLastValidQuery(requestQuery);

        // we have a valid new search
        if (resetSearch) {
          setPage(0);
          setStaleSearchResults(false);
        }

        let allResults = [];
        if (resetSearch) {
          allResults = reqResults.data;
        } else {
          allResults = [...results, ...reqResults.data];
        }
        setMaxPage(Math.ceil(allResults.length / limit));
        setResults(allResults);
        // save cursor or if no more results then set search complete boolean
        if (reqResults.cursor) {
          setCursor(reqResults.cursor);
          setHasMoreSearchResults(true);
        } else {
          setCursor(null);
          setHasMoreSearchResults(false);
        }
      }
      // after results have been returned we can say that we had a valid query
      // to help alert when no results were returned
      setHasQuery(true);
      setSearching(false);
    }
  };

  // Update the search when any search parameter changes, after waiting for 0.5 seconds
  // this short delay allows the search to seem interactive/response without hammering the
  // API for every character change of the search query.
  useEffect(() => {
    const intervalId = setInterval(() => {
      if (query) {
        getResults(true);
      } else {
        setHasQuery(false);
        setStaleSearchResults(false);
        setResults([]);
      }
      updateURLSearchParams();
      clearInterval(intervalId);
    }, 500);
    return () => clearInterval(intervalId);
  }, [query, limit, groups, startDate, endDate]);

  // Trigger getting more results when paging past end of already
  // retrieved results
  useEffect(() => {
    if (page == maxPage) {
      getResults(false);
    }
  }, [page]);

  // get search string and user groups from URL query params
  // we do this after userInfo changes so we know a user's group membership
  useEffect(() => {
    readURLSearchParams();
  }, [userInfo]);

  // add limit to limit options if it is not one of the defaults
  let limitOptions = [10, 25, 50, 100];
  if (limit != 0 && !limitOptions.includes(limit)) {
    limitOptions.push(parseInt(limit));
    limitOptions = limitOptions.sort(function (a, b) {
      return a - b;
    });
  }

  return (
    <Stack>
      <div className="d-flex flex-row justify-content-center">
        <Form className="search-bar">
          <Form.Control
            className="text-center"
            type="text"
            value={query}
            placeholder="search analysis results"
            onChange={(e) => {
              setStaleSearchResults(true && hasQuery);
              setQuery(String(e.target.value));
            }}
            onKeyDown={(e) => {
              e.key === 'Enter' && e.preventDefault();
            }}
          />
        </Form>
      </div>
      <div className="mt-2 d-flex justify-content-center">
        <OverlayTipRight tip={`${hideFilters ? 'Expand' : 'Hide'} search filters`}>
          <Button variant="" className="m-2 clear-btn" onClick={() => setHideFilters(!hideFilters)}>
            <FaFilter size="24" />
          </Button>
        </OverlayTipRight>
      </div>
      <LoadingSpinner loading={searching}></LoadingSpinner>
      {!hideFilters && (
        <Card className="basic-card panel">
          <Row>
            <Col className="d-flex justify-content-center hide-groups mt-3">
              <b>Groups</b>
            </Col>
          </Row>
          <Row className="mt-2">
            <Col className="d-flex justify-content-center groups-col hide-groups">
              <SelectGroups groups={groups} setGroups={setGroups} disabled={searching} />
            </Col>
          </Row>
          <Row className="mt-3">
            <Col className="d-flex justify-content-end">
              <b className="hide-element">Oldest Result</b>
              <b className="hide-small-element">Oldest</b>
            </Col>
            <Col className="d-flex justify-content-start">
              <DatePicker
                maxDate={startDate != null ? startDate : maxDate}
                selected={endDate}
                onChange={(date) => {
                  setEndDate(date);
                }}
              />
            </Col>
          </Row>
          <Row className="mt-1">
            <Col className="d-flex justify-content-end">
              <b className="hide-element">Newest Result</b>
              <b className="hide-small-element">Newest</b>
            </Col>
            <Col className="d-flex justify-content-start">
              <DatePicker
                maxDate={maxDate}
                minDate={endDate}
                selected={startDate}
                onChange={(date) => {
                  setStartDate(date);
                }}
              />
            </Col>
          </Row>
          <Row className="m-3">
            <Col className="d-flex justify-content-end">
              <b>Per Page</b>
            </Col>
            <Col className="d-flex justify-content-start">
              <Form.Select
                className="m-0 limit-field"
                value={limit}
                onChange={(e) => {
                  setLimit(parseInt(e.target.value));
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
      {searchError && hasQuery && (
          <Alert variant="danger" className="d-flex justify-content-center mt-1 mb-0">
            {searchError}
          </Alert>
      )}
      {staleSearchResults && results && results.length > 0 && (
          <Alert variant="info" className="d-flex justify-content-center mt-1 mb-0">
            Showing results for previous search
          </Alert>
      )}
      {results && results.length == 0 && hasQuery && !searchError && (
          <Alert variant="info" className="d-flex justify-content-center mt-1 mb-0">
            No matching results found
          </Alert>
      )}
      {hasQuery && (
          <Card className="basic-card panel">
            <Card.Body>
              <Row>
                <Col className="d-flex justify-content-center sha256-col">SHA256</Col>
                <Col className="d-flex justify-content-center groups-col hide-element">Group(s)</Col>
              </Row>
            </Card.Body>
          </Card>
      )}
      {results.slice(page * limit, page * limit + limit).map((result, resultIdx) => (
        <Row className="result-sha" key={`${resultIdx}_${getSha256(result.id)}`}>
          <Card className="panel">
            <Row>
              {/* add common relative spacing for sha and group name*/}
              <Col className="d-flex justify-content-center sha256-col">
                <Link className="hide-element-sha" to={`/file/${getSha256(result.id)}`}>
                  {getSha256(result.id)}
                </Link>
                <Link className="hide-small-element-sha" to={`/file/${getSha256(result.id)}`}>
                  {getSha256(result.id).substr(0, 10)}
                </Link>
              </Col>
              <Col className="d-flex justify-content-center groups-col hide-element ">{getGroup(result.id)}</Col>
              <hr />
            </Row>
            {result.highlight &&
              Object.keys(result.highlight).map(
                (key) =>
                  key != 'group' && (
                    <Row key={`${getSha256(result.id)}_${resultIdx}_${key}`}>
                      <Col>
                        <span>
                          {key}: {highlightResult(result.highlight[key])}
                        </span>
                      </Col>
                    </Row>
                  ),
              )}
          </Card>
        </Row>
      ))}
      {results.length > 0 && (
        <Row className="pt-4">
          <Col className="d-flex justify-content-center">
            <Pagination>
              <Pagination.Item onClick={() => setPage(page - 1)} disabled={page == 0}>
                Back
              </Pagination.Item>
              <Pagination.Item onClick={() => setPage(page + 1)} disabled={!hasMoreSearchResults && page + 1 >= maxPage}>
                Next
              </Pagination.Item>
            </Pagination>
          </Col>
        </Row>)}
    </Stack>
  );
};

export default Search;
