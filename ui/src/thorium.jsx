import React, { lazy, Fragment, Suspense, useEffect } from 'react';
import { BrowserRouter, Link, Navigate, NavLink, Routes, Route, useNavigate } from 'react-router-dom';
import { Col, Nav, Navbar, NavDropdown, Row } from 'react-bootstrap';
import { FaCube, FaFolderOpen, FaQuestion, FaSitemap, FaUsers, FaUser, FaUpload, FaCog, FaChartLine } from 'react-icons/fa';
import { ErrorBoundary } from 'react-error-boundary';
import { ToastContainer } from 'react-toastify';
import 'react-toastify/dist/ReactToastify.css';

// project imports
import { AuthProvider, RequireAdmin, RequireAuth, useAuth, getApiUrl } from '@utilities';
import { StateAlerts, RenderErrorAlert, OverlayTipRight } from '@components';
import '@styles/main.scss';

// import pages lazily
const SystemStatsContainer = lazy(async () => await import('@pages/system_stats'));
const SystemSettingsContainer = lazy(async () => await import('@pages/system_settings'));
const HomeContainer = lazy(async () => import('@pages/home'));
const NotFoundContainer = lazy(async () => import('@pages/not_found'));
const FileDetailsContainer = lazy(async () => await import('@pages/file_details'));
const ProfileContainer = lazy(async () => await import('@pages/profile'));
const UploadFilesContainer = lazy(async () => await import('@pages/upload_files'));
const FilesBrowsingContainer = lazy(async () => await import('@pages/file_browsing'));
const RepoBrowsingContainer = lazy(async () => await import('@pages/repo_browsing'));
const PipelinesContainer = lazy(async () => await import('@pages/pipelines'));
const ImagesContainer = lazy(async () => await import('@pages/images'));
const GroupsContainer = lazy(async () => await import('@pages/groups'));
const UsersContainer = lazy(async () => await import('@pages/users'));
const ReactionStatus = lazy(async () => await import('@pages/reaction_status'));
const ReactionStageLogs = lazy(async () => await import('@pages/reaction_stage_logs'));
const CreateImageContainer = lazy(async () => await import('@pages/create_image'));
const LoginContainer = lazy(async () => await import('@pages/login'));
// Data loading ui empty for now
const FallbackView = <h1 />;

const NavBanner = () => {
  const { userInfo, logout } = useAuth();
  const navigate = useNavigate();
  const apiURL = getApiUrl();

  // call auth logout and redirect to login
  const handleLogout = () => {
    // re-render navbar to remove username
    logout().then(() => {
      navigate('/');
      // force reload of site on logout
      window.location.reload(true);
    });
  };

  return (
    <Navbar className="navbar-banner panel d-flex justify-content-end">
      <Nav.Link className="home-item" as={Link} to="/">
        <OverlayTipRight tip={"Home"}>
          <img src="/ferris-scientist.png" alt="FerrisScientist" width="40px" />
        </OverlayTipRight>
      </Nav.Link>
      <Nav className="d-flex justify-content-end">
        {<Navbar.Brand className="navbanner-item mx-2 px-2 pb-3" href={`${apiURL}/docs/user/index.html`}>
          <FaQuestion className="mt-2" size={22} />
        </Navbar.Brand>}
        {userInfo && userInfo.username && (
          <NavDropdown align="end" className="navbanner-item" title={`@${userInfo.username}`}>
            <NavDropdown.Item as={Link} to="/profile">
              Profile
            </NavDropdown.Item>
            <NavDropdown.Item onClick={() => handleLogout()}>Logout</NavDropdown.Item>
          </NavDropdown>
        )}
      </Nav>
    </Navbar>
  );
};

const Sidebar = ({ userInfo }) => {
  const SidebarItem = ({ to, short, full }) => {
    return (
      <NavLink
        to={to} // no-decoration
        className={(navData) => (navData.isActive ? 'activeNavLink' : 'navLink')}
      >
        <Row className="reduce-sidebar">
          <Col xs="auto" className="short">
            <OverlayTipRight tip={full}>
              {short}
            </OverlayTipRight>
          </Col>
        </Row>
        <Row className="expand-sidebar">
          <Col xs="auto" className="short">{short}</Col>
          <Col>{full}</Col>
        </Row>
      </NavLink>

    );
  };

  return (
    <div className="sidebar nav-panel">
      <div className="sidebar-items">
        {userInfo && userInfo.role && (
          <Fragment>
            <SidebarItem to="/files" short={<FaFolderOpen size={25} />} full={'Files'} />
            <SidebarItem to="/upload" short={<FaUpload size={25} />} full={'Upload'} />
            <SidebarItem to="/pipelines" short={<FaSitemap size={25} />} full={'Pipelines'} />
            <SidebarItem to="/images" short={<FaCube size={25} />} full={'Images'} />
            <SidebarItem to="/groups" short={<FaUsers size={25} />} full={'Groups'} />
            {userInfo.role == 'Admin' && <SidebarItem to="/users" short={<FaUser size={25} />} full={'Users'} />}
            {userInfo.role == 'Admin' && <SidebarItem to="/stats" short={<FaChartLine size={25} />} full={'Stats'} />}
            {userInfo.role == 'Admin' && <SidebarItem to="/settings" short={<FaCog size={25} />} full={'Settings'} />}
          </Fragment>
        )}
      </div>
    </div>
  );
};

const SidebarColumn = () => {
  const { userInfo } = useAuth();
  if (userInfo && userInfo.token) {
    return (
      <Col className="sidebar-column">
        <RequireAuth>
          <Sidebar userInfo={userInfo} />
        </RequireAuth>
      </Col>
    );
  } else {
    return null;
  }
};

const Page = ({ auth, admin, Page }) => {
  const { refreshUserInfo } = useAuth();
  // Check to see if user info is stale on first page load
  useEffect(() => {
    refreshUserInfo();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  });

  // Require an authed user with Admin role
  if (admin) {
    return (
      <RequireAuth>
        <RequireAdmin>
          <StateAlerts />
          <Page />
        </RequireAdmin>
      </RequireAuth>
    );
    // Require user to be authed
  } else if (auth) {
    return (
      <RequireAuth>
        <StateAlerts />
        <Page />
      </RequireAuth>
    );
    // No page auth or Admin role required
  } else {
    return (
      <>
        <StateAlerts />
        <Page />
      </>
    );
  }
};

const Thorium = () => {
  return (
    <BrowserRouter>
      <AuthProvider>
        {/* eslint-disable-next-line react/no-unknown-property*/}
        <div className="body">
          <ToastContainer
            position="top-right"
            autoClose={2000}
            hideProgressBar={true}
            newestOnTop={true}
            closeOnClick={true}
            pauseOnHover={false}
            draggable={false}
            rtl={false}
            theme="dark"
          />
          <Row>
            <NavBanner />
          </Row>
          <Row>
            <SidebarColumn />
            <Col className="page">
              <Suspense fallback={FallbackView}>
                <ErrorBoundary fallback={<RenderErrorAlert />}>
                  <Routes>
                    <Route exact path="/files" element={<Page auth={true} Page={FilesBrowsingContainer} />} />
                    <Route exact path="/file" element={<Page auth={true} Page={FileDetailsContainer} />} />
                    <Route exact path="/file/:sha256" element={<Page auth={true} Page={FileDetailsContainer} />} />
                    <Route exact path="/files/:sha256" element={<Page auth={true} Page={FileDetailsContainer} />} />
                    <Route exact path="/reaction/:group/:reactionID" element={<Page auth={true} Page={ReactionStatus} />} />
                    <Route exact path="/reactions/:group/:reactionID" element={<Page auth={true} Page={ReactionStatus} />} />
                    <Route exact path="/reaction/logs/:group/:reactionID/:stage" element={<Page auth={true} Page={ReactionStageLogs} />} />
                    <Route exact path="/reactions/logs/:group/:reactionID/:stage" element={<Page auth={true} Page={ReactionStageLogs} />} />
                    <Route exact path="/profile" element={<Page auth={true} Page={ProfileContainer} />} />
                    <Route exact path="/stats" element={<Page auth={true} Page={SystemStatsContainer} />} />
                    <Route exact path="/repos" element={<Page auth={true} Page={RepoBrowsingContainer} />} />
                    <Route exact path="/upload" element={<Page auth={true} Page={UploadFilesContainer} />} />
                    <Route exact path="/pipelines" element={<Page auth={true} Page={PipelinesContainer} />} />
                    <Route exact path="/images" element={<Page auth={true} Page={ImagesContainer} />} />
                    <Route exact path="/groups" element={<Page auth={true} Page={GroupsContainer} />} />
                    <Route exact path="/users" element={<Page admin={true} Page={UsersContainer} />} />
                    <Route exact path="/settings" element={<Page admin={true} Page={SystemSettingsContainer} />} />
                    <Route exact path="/create/image" element={<Page auth={true} Page={CreateImageContainer} />} />
                    <Route exact path="/auth" element={<Page auth={false} Page={LoginContainer} />} />
                    <Route exact path="/" element={<Page auth={true} Page={HomeContainer} />} />
                    <Route path="*" element={<Page auth={true} Page={NotFoundContainer} />} />
                    <Route index element={<Page auth={true} Page={HomeContainer} />} />
                  </Routes>
                </ErrorBoundary>
              </Suspense>
            </Col>
          </Row>
        </div>
      </AuthProvider>
    </BrowserRouter>
  );
};

export default Thorium;
