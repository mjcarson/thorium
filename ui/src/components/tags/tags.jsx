import React, { Fragment, useState } from 'react';
import { Alert, Button, Modal } from 'react-bootstrap';
import { FaPlus } from 'react-icons/fa';

// project imports
import { OverlayTipBottom } from '@components';
import CondensedTags from './condensed_tags';
import EditableTags from './editable_tags';

// filter tags to only include specific tags
const filterIncludedTags = (tags, includeList) => {
  const upperIncludeList = includeList.map((tag) => {
    return tag.toUpperCase();
  });
  if (tags) {
    return Object.fromEntries(Object.entries(tags).filter(([k, v]) => upperIncludeList.includes(k.toUpperCase())));
  }
  return [];
};

// return tags without exluded values
const filterExcludedTags = (tags, excludeList) => {
  const upperExcludedList = excludeList.map((tag) => {
    return tag.toUpperCase();
  });
  return Object.fromEntries(Object.entries(tags).filter(([k, v]) => !upperExcludedList.includes(k.toUpperCase())));
};

// Lists of preformatted or categorized tags
const FileInfoTagKeys = [
  'FileType',
  'FileTypeExtension',
  'Match',
  'FileTypeMatch',
  'Format',
  'FileFormat',
  'Compiler',
  'CompilerVersion',
  'CompilerFlags',
  'FileSize',
  'Arch',
  'Endianess',
  'PEType',
  'MachineType',
  'MIMEType',
  'EntryPoint',
  'linker',
  'packer',
  'type',
  'tool',
  'imphash',
  'detections',
  'Sign tool',
  'SignTool',
];
const TLPLevels = ['CLEAR', 'GREEN', 'AMBER', 'AMBER+STRICT', 'RED'];
const DangerTagKeys = [
  'SYMANTECAV',
  'CLAMAV',
  'YARARULEHITS',
  'YARAHIT',
  'SURICATASIGHIT',
  'SURICATAALERT',
  'IDSALERT',
  'PACKED',
  'POLYGLOT',
];
// need capitalized file info keys for value checks (all keys cast to uppercase)
const FormattedFileInfoTagKeys = FileInfoTagKeys.map((tag) => tag.toUpperCase());

const AlertBanner = ({ prefix, errorStatus }) => {
  const [show, setShow] = useState(true);
  return (
    <Fragment>
      {show && (
        <Alert className="d-flex justify-content-center" onClose={() => setShow(false)} variant="danger">
          {prefix}: {errorStatus}
        </Alert>
      )}
    </Fragment>
  );
};

const AddTagBadge = ({ tag, setTag, value, setValue, addTagCallback, tagClass, defaultTag }) => {
  const [adding, setAdding] = useState(false);
  return (
    <>
      {adding && (
        <div className={`tag-item ${tagClass}`}>
          <input className={`tag-input tag-item ${tagClass}`} value={tag} onChange={(e) => setTag(e.target.value)}></input>
          {`: `}
          <input className={`tag-input tag-item ${tagClass}`} value={value} onChange={(e) => setValue(e.target.value)}></input>
        </div>
      )}
      <Button
        className="p-1 m-1 add-tag-btn"
        onClick={() => {
          setAdding(true);
          if (tag != '' && value != '') {
            addTagCallback(tag, value);
            defaultTag ? setTag(defaultTag) : setTag('');
            setValue('');
          }
        }}
      >
        <FaPlus size="24" />
      </Button>
    </>
  );
};

const TagBadge = ({ tag, value, condensed, action }) => {
  const [showRedirectModal, setShowRedirectModal] = useState(false);
  let badgeClass = '';
  let tagText = '';

  const upperTag = tag.toUpperCase();
  // format traffic light protocol tags
  if (upperTag == 'TLP') {
    // on details page, only print value because TLP is in a different col
    if (!condensed) {
      tagText = value.toUpperCase();
    } else {
      tagText = `TLP: ${value.toUpperCase()}`;
    }
    switch (value.toUpperCase()) {
      case 'RED':
        badgeClass = 'tlp-red-btn';
        break;
      case 'AMBER':
        badgeClass = 'tlp-amber-btn';
        break;
      case 'AMBER+STRICT':
        badgeClass = 'tlp-amber-btn';
        break;
      case 'GREEN':
        badgeClass = 'tlp-green-btn';
        break;
      case 'WHITE':
        badgeClass = 'tlp-clear-btn';
        break;
      case 'CLEAR':
        badgeClass = 'tlp-clear-btn';
        break;
    }
  } else if (upperTag == 'RESULTS') {
    badgeClass = 'general-tag';
    tagText = `${tag}: ${value}`;
  } else if (upperTag == 'ATT&CK') {
    badgeClass = 'attack-tag';
    tagText = `${value}`;
  } else if (upperTag == 'MBC') {
    badgeClass = 'mbc-tag';
    tagText = `${value}`;
  } else if (FormattedFileInfoTagKeys.includes(upperTag)) {
    badgeClass = 'info-tag';
    tagText = `${tag}: ${value}`;
  } else {
    if (DangerTagKeys.includes(tag.toUpperCase())) {
      badgeClass = 'danger-tag';
    } else {
      badgeClass = 'other-tag';
    }
    tagText = `${tag}: ${value}`;
  }
  // returned rendered component
  if (action == 'scroll') {
    const scrollToResult = (value) => {
      document.getElementById(`results-tab-${value}`).scrollIntoView();
    };
    return (
      <OverlayTipBottom tip={`Click to jump to ${value} results`}>
        <div className={`${badgeClass} ms-1 mb-1 tag-item clickable tags-hide`} onClick={() => scrollToResult(value)}>
          {tagText}
        </div>
        <div className={`${badgeClass} ms-1 mb-1 tag-item clickable short-tag`} onClick={() => scrollToResult(value)}>
          {tagText.length > 30 ? tagText.substring(0, 30) + '...' : tagText}
        </div>
      </OverlayTipBottom>
    );
    // link to external mitre docs for Att&ck tags
  } else if (action == 'docs' && upperTag == 'ATT&CK') {
    const tactic = value.split(' ');
    const attackID = tactic.at(-1).split('.')[0];
    const attackSubID = tactic.at(-1).split('.').at(1);
    let redirectURL = '';
    if (attackSubID != undefined) {
      redirectURL = `https://attack.mitre.org/techniques/${attackID}/${attackSubID}/`;
    } else {
      redirectURL = `https://attack.mitre.org/techniques/${attackID}/`;
    }
    // on click function to redirect to external URL
    const redirectToExternal = () => {
      window.open(redirectURL, '_blank');
    };
    return (
      <>
        <Modal show={showRedirectModal} onHide={() => setShowRedirectModal(false)}>
          <Modal.Header closeButton>
            <h3>Navigate to an external site?</h3>
          </Modal.Header>
          <Modal.Body className="d-flex justify-content-center">
            <i>{redirectURL}</i>
          </Modal.Body>
          <Modal.Footer className="d-flex justify-content-center">
            <Button
              variant=""
              className="warning-btn"
              onClick={() => {
                redirectToExternal();
                setShowRedirectModal(false);
              }}
            >
              Confirm
            </Button>
          </Modal.Footer>
        </Modal>
        <OverlayTipBottom tip={`Click to see mitre documentation on this technique: ${tagText}`}>
          <a className="no-decoration" onClick={() => setShowRedirectModal(true)}>
            <div className={`${badgeClass} ms-1 mb-1 tag-item clickable tags-hide`}>{tagText}</div>
            <div className={`${badgeClass} ms-1 mb-1 tag-item clickable short-tag`}>
              {tagText.length > 30 ? tagText.substring(0, 30) + '...' : tagText}
            </div>
          </a>
        </OverlayTipBottom>
      </>
    );
    // link to external mitre docs for MBC tags
  } else if (action == 'docs' && upperTag == 'MBC') {
    const splitIndex = value.lastIndexOf(' ');
    const identifier = value.slice(splitIndex);
    const splitText = value.slice(0, splitIndex).split('::');
    const behavior = splitText[0].replaceAll(' ', '-').toLowerCase();
    const method = splitText[1].replaceAll(' ', '-').toLowerCase();
    let redirectURL = '';
    if (!identifier.includes('C')) {
      redirectURL = `https://github.com/MBCProject/mbc-markdown/tree/v3.0/${behavior}/${method}.md`;
    } else {
      redirectURL = `https://github.com/MBCProject/mbc-markdown/tree/v3.0/micro-behaviors/${behavior}/${method}.md`;
    }
    // on click function to redirect to external URL
    const redirectToExternal = () => {
      window.open(redirectURL, '_blank');
    };

    return (
      <>
        <Modal show={showRedirectModal} onHide={() => setShowRedirectModal(false)}>
          <Modal.Header closeButton>
            <h3>Navigate to an external site?</h3>
          </Modal.Header>
          <Modal.Body className="d-flex justify-content-center">
            <i>{redirectURL}</i>
          </Modal.Body>
          <Modal.Footer className="d-flex justify-content-center">
            <Button
              variant=""
              className="warning-btn"
              onClick={() => {
                redirectToExternal();
                setShowRedirectModal(false);
              }}
            >
              Confirm
            </Button>
          </Modal.Footer>
        </Modal>
        <OverlayTipBottom tip={`Click to see mitre documentation on this behavior: ${tagText}`}>
          <a className="no-decoration" onClick={() => setShowRedirectModal(true)}>
            <div className={`${badgeClass} ms-1 mb-1 tag-item clickable tags-hide`}>{tagText}</div>
            <div className={`${badgeClass} ms-1 mb-1 tag-item clickable short-tag`}>
              {tagText.length > 30 ? tagText.substring(0, 30) + '...' : tagText}
            </div>
          </a>
        </OverlayTipBottom>
      </>
    );
  } else if (action == 'link') {
    // built the URL search query params from the tag key and value
    return (
      <OverlayTipBottom tip={`Click to browse files with tag: ${tagText}`}>
        <a
          className="no-decoration"
          onClick={() => {
            // we are already browsing and want to append tags to current search params
            if (window.location.pathname.startsWith("/files")) {
              const query = new URLSearchParams(window.location.search);
              query.append(`tags[${tag}]`, value);
              window.location.href = `/files?${query.toString()}`;
              console.log(query);
            } else {
              const query = new URLSearchParams();
              query.append('limit', "10");
              query.append(`tags[${tag}]`, value);
              window.location.href = `/files?${query.toString()}`;
            }
          }}
        >
          <div className={`${badgeClass} ms-1 mb-1 tag-item clickable tags-hide`}>{tagText}</div>
          <div className={`${badgeClass} ms-1 mb-1 tag-item clickable short-tag`}>
            {tagText.length > 30 ? tagText.substring(0, 30) + '...' : tagText}
          </div>
        </a>
      </OverlayTipBottom>
    );
  } else {
    return (
      <div>
        <div className={`${badgeClass} ms-1 mb-1 tag-item tags-hide`}>{tagText}</div>
        <div className={`${badgeClass} ms-1 mb-1 tag-item short-tag`}>
          {tagText.length > 30 ? tagText.substring(0, 30) + '...' : tagText}
        </div>
      </div>
    );
  }
};

export {
  AlertBanner,
  AddTagBadge,
  CondensedTags,
  EditableTags,
  FileInfoTagKeys,
  filterIncludedTags,
  filterExcludedTags,
  FormattedFileInfoTagKeys,
  TagBadge,
  TLPLevels,
};
