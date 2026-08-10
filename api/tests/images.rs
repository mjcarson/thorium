//! Tests the Images routes in Thorium

use std::collections::HashSet;
use std::path::PathBuf;

use futures::{StreamExt, TryStreamExt, stream};
use thorium::models::{
    ArgStrategy, AutoTagLogic, AutoTagUpdate, BurstableResourcesUpdate,
    CacheDependencySettingsUpdate, ChildFilters, ChildFiltersUpdate,
    ChildrenDependencySettingsUpdate, CleanupUpdate, DependenciesUpdate, DependencyPassStrategy,
    EphemeralDependencySettingsUpdate, FileNamingStrategy, FilesHandlerUpdate,
    GenericCacheDependencySettingsUpdate, GroupUpdate, GroupUsersUpdate, HostPathWhitelistUpdate,
    ImageArgsUpdate, ImageBan, ImageBanKind, ImageBanUpdate, ImageLifetime,
    ImageNetworkPolicyUpdate, ImageScaler, ImageUpdate, ImageVersion, KvmUpdate,
    NetworkPolicyRequest, NotificationLevel, NotificationParams, NotificationRequest,
    OutputCollectionUpdate, OutputDisplayType, OutputHandler, PipelineRequest,
    RepoDependencySettingsUpdate, Resources, ResourcesUpdate, ResultDependencySettingsUpdate,
    SampleDependencySettingsUpdate, SecurityContextUpdate, SpawnLimits, SystemSettingsResetParams,
    SystemSettingsUpdate, SystemSettingsUpdateParams, TagDependencySettingsUpdate, Volume,
    VolumeTypes,
};
use thorium::test_utilities::{self, generators};
use thorium::{Error, contains, fail, is, is_in, unwrap_variant, vec_in_vec};
use uuid::Uuid;

#[tokio::test]
async fn create() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // Create a test image
    let image_req = generators::gen_image(&group);
    let resp = client.images.create(&image_req).await?;
    is!(resp.status().as_u16(), 204);
    Ok(())
}

#[tokio::test]
async fn create_bad_volume_name() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // attempt to create an image with an invalid name
    let image_req = generators::gen_image(&group).volume(Volume::new(
        "InvalidImageName!@#$",
        "/placeholder",
        VolumeTypes::Secret,
    ));
    let resp = client.images.create(&image_req).await;
    // expect a BAD error
    fail!(resp, 400, "volume name must be only lowercase alphanumeric");
    // attempt to create an image with a reserved thorium name
    let image_req = generators::gen_image(&group).volume(Volume::new(
        "thorium-thingy",
        "/placeholder",
        VolumeTypes::Secret,
    ));
    let resp = client.images.create(&image_req).await;
    // expect a BAD error
    fail!(resp, 400, "Volume names cannot start with 'thorium'");
    Ok(())
}

#[tokio::test]
async fn create_conflict() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // create a test image
    let mut image_req = generators::gen_image(&group);
    // set the description as blank
    image_req.description = None;
    let resp = client.images.create(&image_req).await?;
    is!(resp.status().as_u16(), 204);
    // attempt to save the image again, this time with the optional description set
    image_req = image_req.description("This description should not be set for the existing image");
    let resp = client.images.create(&image_req).await;
    // expect a conflict error
    fail!(resp, 409);
    // check that the description was NOT set
    let image = client.images.get(&group, &image_req.name).await?;
    is!(image.description, None::<String>);
    Ok(())
}

#[tokio::test]
async fn create_bad_child_filter() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // attempt to create an image with a bad child filter regular expression
    let image_req = generators::gen_image(&group)
        .child_filters(ChildFilters::default().mime(r"incomplete-escape\"));
    let resp = client.images.create(&image_req).await;
    fail!(resp, 400, "filter regular expressions is invalid");
    Ok(())
}

#[serial_test::serial]
#[tokio::test]
async fn create_host_path() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // reset settings with no scan
    client
        .system
        .reset_settings(&SystemSettingsResetParams::default().no_scan())
        .await?;
    // get user client
    let user_client = generators::client(&client).await?;
    // create a group
    let group = generators::groups(1, &user_client).await?.remove(0).name;
    // attempt to create an image with an un-whitelisted host path
    let image_req = generators::gen_host_path(&group, "/disallowed/mount");
    let resp = user_client.images.create(&image_req).await;
    // expect a 400 error
    fail!(resp, 400, "not in the list of allowed host paths");
    // create that same image as an admin and expect the same result
    let resp = client.images.create(&image_req).await;
    fail!(resp, 400, "not in the list of allowed host paths");
    // update the whitelist with a path
    let whitelisted_path = "/whitelisted/path";
    client
        .system
        .update_settings(
            &SystemSettingsUpdate::default()
                .host_path_whitelist(HostPathWhitelistUpdate::default().add_path(whitelisted_path)),
            &SystemSettingsUpdateParams::default().no_scan(),
        )
        .await?;
    // successfully create an image with the whitelisted host path as a user
    let image_req = generators::gen_host_path(&group, whitelisted_path);
    user_client.images.create(&image_req).await?;
    // successfully create an image with a host path whose parent is in the whitelist
    let image_req =
        generators::gen_host_path(&group, format!("{whitelisted_path}/child/grandchild"));
    user_client.images.create(&image_req).await?;
    // allow unrestricted host paths
    client
        .system
        .update_settings(
            &SystemSettingsUpdate::default().allow_unrestricted_host_paths(true),
            &SystemSettingsUpdateParams::default().no_scan(),
        )
        .await?;
    // successfully create an image with a host path not on the whitelist as a user
    let image_req = generators::gen_host_path(&group, "/not/whitelisted/but/unrestricted");
    user_client.images.create(&image_req).await?;
    // attempt to create a relative (and therefore invalid) host path
    let image_req = generators::gen_host_path(&group, "relative/so/invalid");
    let resp = user_client.images.create(&image_req).await;
    fail!(resp, 400, "Host paths must be absolute");
    // attempt to create an invalid host path with relative traversal
    let image_req = generators::gen_host_path(&group, "/absolute/but/has/...../so/bad");
    let resp = user_client.images.create(&image_req).await;
    fail!(resp, 400, "must not contain relative traversal");
    // disallow unrestricted host paths again
    client
        .system
        .update_settings(
            &SystemSettingsUpdate::default().allow_unrestricted_host_paths(false),
            &SystemSettingsUpdateParams::default().no_scan(),
        )
        .await?;
    // make sure we can't mount to un-whitelisted paths after we restrict host paths again
    let image_req = generators::gen_host_path(&group, "/not/whitelisted/but/unrestricted");
    let resp = user_client.images.create(&image_req).await;
    fail!(resp, 400, "not in the list of allowed host paths");
    // make sure we can't mount to un-whitelisted paths after we restrict host paths again
    let image_req = generators::gen_host_path(&group, "/not/whitelisted/but/unrestricted");
    let resp = user_client.images.create(&image_req).await;
    fail!(resp, 400, "not in the list of allowed host paths");
    // reset settings
    client
        .system
        .reset_settings(&SystemSettingsResetParams::default().no_scan())
        .await?;
    Ok(())
}

#[tokio::test]
async fn create_network_policy() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // create groups
    let groups = generators::groups(2, &client)
        .await?
        .into_iter()
        .map(|g| g.name)
        .collect::<Vec<String>>();
    // create network policies in those groups
    let network_policies = generators::network_policies(&groups, 2, &client).await?;
    // create an image with those network policies that's in one of the policies' groups
    let mut image_req = generators::gen_image(&groups[0]);
    image_req = image_req.network_policies(network_policies.into_iter().map(|p| p.name));
    let resp = client.images.create(&image_req).await?;
    is!(resp.status().as_u16(), 204);
    Ok(())
}

#[tokio::test]
async fn create_bad_network_policy() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let image_group = generators::groups(1, &client).await?.remove(0).name;
    // create an image with a network policy that doesn't exist
    let mut image_req = generators::gen_image(&image_group);
    image_req = image_req.network_policy("network-policy-no-exist");
    let resp = client.images.create(&image_req).await;
    fail!(resp, 404);
    // create an image with a network policy that's not in the image's group
    let policy_group = generators::groups(1, &client).await?.remove(0).name;
    let network_policy = generators::network_policies(&[policy_group.clone()], 1, &client)
        .await?
        .remove(0);
    image_req = image_req.network_policy(&network_policy.name);
    let resp = client.images.create(&image_req).await;
    fail!(resp, 404);
    // try to create a new image not scaled by K8's but with a network policy
    let image_req = generators::gen_ext_image(&policy_group).network_policy(&network_policy.name);
    let resp = client.images.create(&image_req).await;
    fail!(resp, 400, "only be applied to images scaled in K8s");
    Ok(())
}

#[tokio::test]
async fn create_default_network_policies() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // create group
    let group = generators::groups(1, &client).await?.remove(0).name;
    let group_array = [group.clone()];
    // create default network policies in that group
    let default_policy_reqs: Vec<NetworkPolicyRequest> = (0..10)
        .map(|_| {
            let mut req = generators::gen_network_policy(&group_array);
            req.default_policy = true;
            req
        })
        .collect();
    stream::iter(default_policy_reqs.iter().cloned())
        .map(Ok::<NetworkPolicyRequest, thorium::Error>)
        .try_for_each_concurrent(50, |req| {
            let client_ref = &client;
            async move {
                client_ref.network_policies.create(req).await?;
                Ok(())
            }
        })
        .await?;
    // create an image with no network policies
    let mut image_req = generators::gen_image(&group);
    image_req.network_policies = HashSet::new();
    client.images.create(&image_req).await?;
    // make sure that image has all default network policies
    let image = client.images.get(&group, &image_req.name).await?;
    for policy_name in default_policy_reqs.iter().map(|policy| &policy.name) {
        contains!(image.network_policies, policy_name);
    }
    Ok(())
}

#[tokio::test]
async fn create_kvm() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // create an image that is scaled by the kvm scaler
    let image_req = generators::gen_kvm_image(&group);
    let resp = client.images.create(&image_req).await?;
    is!(resp.status().as_u16(), 204);
    // get the image and compare it
    let retrieved = client.images.get(&group, &image_req.name).await?;
    is!(retrieved, image_req);
    // make sure our kvm settings were saved
    let kvm = retrieved
        .kvm
        .ok_or_else(|| Error::new("Created kvm image has no kvm settings"))?;
    let requested = image_req
        .kvm
        .ok_or_else(|| Error::new("Generated kvm image request has no kvm settings"))?;
    is!(kvm.xml, requested.xml);
    is!(kvm.qcow2, requested.qcow2);
    Ok(())
}

#[tokio::test]
async fn get() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // create an image and then get it
    let image = generators::gen_image(&group);
    let resp = client.images.create(&image).await?;
    is!(resp.status().as_u16(), 204);
    // get the image and compare it
    let retrieved = client.images.get(&group, &image.name).await?;
    is!(retrieved, image);
    Ok(())
}

#[tokio::test]
async fn list() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup 20 random images
    let images = generators::images(&group, 20, false, &client).await?;
    // get the names of all the images we have created
    let names: Vec<String> = images.iter().map(|images| images.name.clone()).collect();
    // list the images we just created
    let mut cursor = client.images.list(&group);
    cursor.next().await?;
    // make sure all the images we tried to create are in our list
    for image in names {
        is_in!(cursor.names, image);
    }
    Ok(())
}

#[tokio::test]
async fn list_details() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup 20 random images
    let images = generators::images(&group, 20, false, &client).await?;
    // list theimages we just created
    let mut cursor = client.images.list(&group).details();
    cursor.next().await?;
    // make sure all the group details we tried to create are in our list
    vec_in_vec!(&cursor.details, &images);
    Ok(())
}

#[tokio::test]
async fn list_pagination() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup enough images that we could need more then one page to list them all
    let images = generators::images(&group, 30, false, &client).await?;
    // list the images we just created with a page size well below the total image count
    let mut cursor = client.images.list(&group).page_size(7);
    // crawl this cursor to exhaustion and collect every name it returns
    let mut names = HashSet::with_capacity(images.len());
    while !cursor.exhausted {
        cursor.next().await?;
        names.extend(cursor.names.drain(..));
    }
    // the page size is only a hint to the backend so we can't check how many pages we crawled,
    // but every image in this group should have been returned exactly once across all of them
    is!(names.len(), images.len());
    // make sure all the images we tried to create are in our list
    for image in &images {
        contains!(names, &image.name);
    }
    Ok(())
}

#[tokio::test]
async fn list_details_pagination() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup enough images that we could need more then one page to list them all
    let images = generators::images(&group, 30, false, &client).await?;
    // list the images we just created with a page size well below the total image count
    let mut cursor = client.images.list(&group).details().page_size(7);
    // crawl this cursor to exhaustion and collect every image it returns
    let mut details = Vec::with_capacity(images.len());
    while !cursor.exhausted {
        cursor.next().await?;
        details.append(&mut cursor.details);
    }
    // every image in this group should have been returned exactly once across all pages
    is!(details.len(), images.len());
    // make sure all the group details we tried to create are in our list
    vec_in_vec!(&details, &images);
    Ok(())
}

#[tokio::test]
async fn list_limit() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup more images then we are going to ask our cursor for
    generators::images(&group, 30, false, &client).await?;
    // set a limit well below the number of images in this group
    let limit = 5;
    // crawl this limited cursor until it stops handing back new pages
    let mut cursor = client.images.list(&group).page_size(2).limit(limit);
    while !cursor.exhausted {
        cursor.next().await?;
    }
    // the limit is only weakly enforced by the backend so all we can check is that our cursor
    // stopped once it had retrieved at least as many images as we asked for
    is!((cursor.retrieved >= limit), true);
    Ok(())
}

#[tokio::test]
async fn list_unauthorized() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    generators::images(&group, 1, false, &client).await?;
    // get a user client for a user that is not in this group
    let user_client = generators::client(&client).await?;
    // this user cannot see any images in a group they are not a member of
    let mut cursor = user_client.images.list(&group);
    let resp = cursor.next().await;
    fail!(resp, 401);
    // this user cannot see any image details in a group they are not a member of either
    let mut cursor = user_client.images.list(&group).details();
    let resp = cursor.next().await;
    fail!(resp, 401);
    Ok(())
}

#[tokio::test]
async fn get_unauthorized() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // get a user client for a user that is not in this group
    let user_client = generators::client(&client).await?;
    // this user cannot get an image in a group they are not a member of
    let resp = user_client.images.get(&group, &image.name).await;
    fail!(resp, 401);
    Ok(())
}

#[tokio::test]
async fn update_unauthorized() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // get a user client for a user that is not in this group
    let user_client = generators::client(&client).await?;
    // this user cannot update an image in a group they are not a member of
    let update = ImageUpdate::default().description("edited description");
    let resp = user_client
        .images
        .update(&group, &image.name, &update)
        .await;
    fail!(resp, 401);
    Ok(())
}

#[tokio::test]
async fn delete_unauthorized() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // get a user client for a user that is not in this group
    let user_client = generators::client(&client).await?;
    // this user cannot delete an image in a group they are not a member of
    let resp = user_client.images.delete(&group, &image.name).await;
    fail!(resp, 401);
    // make sure the image is still there
    client.images.get(&group, &image.name).await?;
    Ok(())
}

#[tokio::test]
async fn delete() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // delete that image
    client.images.delete(&group, &image.name).await?;
    // make sure the image was deleted
    let resp = client.images.get(&group, &image.name).await;
    fail!(resp, 404);
    // TODO: test that all notifications were deleted; can't do that without specific
    // route to get notifications because we get a 404 when we try to get notifications
    // after deletion
    Ok(())
}

#[tokio::test]
async fn delete_conflict() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // build our pipeline request
    let order = serde_json::json!(vec![&image.name]);
    let pipe_req = PipelineRequest::new(&group, "testpipe", order);
    // create a pipeline that uses this image
    client.pipelines.create(&pipe_req).await?;
    // delete that image
    let status = client.images.delete(&group, &image.name).await;
    fail!(status, 409);
    Ok(())
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn update() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // create network policies
    let network_policies = generators::network_policies(&[group.clone()], 2, &client).await?;
    let default_policy = generators::gen_network_policy(&[group.clone()]).default_policy();
    client
        .network_policies
        .create(default_policy.clone())
        .await?;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // build and update for that image
    let update = ImageUpdate::default()
        .version(ImageVersion::SemVer(semver::Version::parse("1.1.0-RC")?))
        .image("rust:1.48.0")
        .lifetime(ImageLifetime::jobs(12))
        .timeout(123_452)
        .resources(
            ResourcesUpdate::default()
                .millicpu(2600)
                .memory("4Gi")?
                .storage("128Gi")?
                .nvidia_gpu(3)
                .amd_gpu(1),
        )
        .add_volume(Volume::new("test-vol", "/files", VolumeTypes::ConfigMap))
        .description("edited description")
        .remove_volume("woot")
        .disable_logs()
        .enable_generator()
        .dependencies(
            DependenciesUpdate::default()
                .samples(
                    SampleDependencySettingsUpdate::default()
                        .location("/updated/path")
                        .kwarg("--update")
                        .strategy(DependencyPassStrategy::Names),
                )
                .ephemeral(
                    EphemeralDependencySettingsUpdate::default()
                        .location("/updated/ephemeral/path")
                        .kwarg("--ephemeral")
                        .strategy(DependencyPassStrategy::Names)
                        .add_name("updated.txt")
                        .remove_name("file.txt"),
                )
                .results(
                    ResultDependencySettingsUpdate::default()
                        .image("new-harvest")
                        .remove_image("harvest")
                        .location("/new/location")
                        .kwarg(thorium::models::KwargDependency::List("--new".to_owned()))
                        .name("new-fields.txt")
                        .remove_name("field.txt"),
                )
                .repos(
                    RepoDependencySettingsUpdate::default()
                        .location("/new/location")
                        .kwarg("--new-repos")
                        .strategy(DependencyPassStrategy::Disabled),
                ),
        )
        .display_type(OutputDisplayType::Json)
        .output_collection(
            OutputCollectionUpdate::default()
                .handler(OutputHandler::Files)
                .files(
                    FilesHandlerUpdate::default()
                        .results("/updated/results")
                        .result_files("/updated/result_files")
                        .tags("/updated/tags")
                        .add_name("corn.csv")
                        .remove_name("corn.json"),
                )
                .auto_tag(
                    "Plant",
                    AutoTagUpdate::default().logic(AutoTagLogic::Equal(serde_json::json!("Corn"))),
                ),
        )
        .child_filters(
            ChildFiltersUpdate::default()
                .add_file_extensions(["exe", "txt", "so"])
                .submit_non_matches(true),
        )
        .clean_up(
            CleanupUpdate::default()
                .script("/updated/script.py".to_owned())
                .job_id(ArgStrategy::Kwarg("--new_job_id".to_owned()))
                .results(ArgStrategy::Append)
                .result_files_dir(ArgStrategy::Kwarg("--output_dir".to_owned())),
        )
        .bans(
            ImageBanUpdate::default()
                .add_ban(ImageBan::new(ImageBanKind::generic("Test ban 1!")))
                .add_ban(ImageBan::new(ImageBanKind::generic("Test ban 2!"))),
        )
        .network_policies(
            ImageNetworkPolicyUpdate::default()
                // add policies
                .add_policies(network_policies.into_iter().map(|p| p.name))
                // remove default policy
                .remove_policy(default_policy.name),
        );
    // update that image and check the response code
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our updates were applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    // make sure that the scaler cache is set to be cleared
    let info = client.system.get_info(Some(ImageScaler::K8s)).await?;
    is!(info.expired_cache(ImageScaler::K8s), true);
    Ok(())
}

#[tokio::test]
async fn update_bad_name() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // build and update for that image
    let update = ImageUpdate::default()
        .image("rust:1.48.0")
        .lifetime(ImageLifetime::jobs(12))
        .timeout(123_452)
        .resources(
            ResourcesUpdate::default()
                .millicpu(2600)
                .memory("4Gi")?
                .storage("128Gi")?
                .nvidia_gpu(3)
                .amd_gpu(1),
        )
        .add_volume(Volume::new("Test_vol**", "/files", VolumeTypes::ConfigMap))
        .remove_volume("woot");
    // update that image and check the response code
    let resp = client.images.update(&group, &image.name, &update).await;
    fail!(resp, 400);
    Ok(())
}

#[tokio::test]
async fn update_user() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // get a user client
    let user_client = generators::client(&client).await?;
    // Create a group
    let group = generators::groups(1, &user_client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &user_client)
        .await?
        .remove(0);
    // build and update for that image
    let update = ImageUpdate::default()
        .image("rust:1.48.0")
        .lifetime(ImageLifetime::jobs(12))
        .timeout(3)
        .resources(
            ResourcesUpdate::default()
                .millicpu(2600)
                .memory("4Gi")?
                .storage("128Gi")?
                .nvidia_gpu(3)
                .amd_gpu(1),
        )
        .add_volume(Volume::new("test-vol", "/files", VolumeTypes::ConfigMap))
        .remove_volume("woot");
    // update that image and check the response code
    user_client
        .images
        .update(&group, &image.name, &update)
        .await?;
    // get the image and make sure our updates were applied
    let updated = user_client.images.get(&group, &image.name).await?;
    is!(updated, update);
    // make sure that the scaler cache is set to be cleared
    let info = client.system.get_info(Some(ImageScaler::K8s)).await?;
    is!(info.expired_cache(ImageScaler::K8s), true);
    Ok(())
}

#[tokio::test]
async fn update_clear_description() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // update the image with some description
    let update = ImageUpdate::default().description("edited description");
    client.images.update(&group, &image.name, &update).await?;
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated.description, update.description);
    // now clear the description with a new ImageUpdate
    let update = ImageUpdate::default().clear_description();
    client.images.update(&group, &image.name, &update).await?;
    let updated = client.images.get(&group, &image.name).await?;
    // ensure that description is empty
    is!(updated.description, Option::<String>::None);
    Ok(())
}

#[tokio::test]
async fn update_clear_version() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // update the image with some version
    let update = ImageUpdate::default().version(ImageVersion::Custom("custom_v1".to_string()));
    client.images.update(&group, &image.name, &update).await?;
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.version, update.version);
    // now clear the version with a new ImageUpdate
    let update = ImageUpdate::default().clear_version();
    client.images.update(&group, &image.name, &update).await?;
    let updated = client.images.get(&group, &image.name).await?;
    // ensure that version is empty
    is!(updated, update);
    is!(updated.version, Option::<ImageVersion>::None);
    Ok(())
}

#[tokio::test]
async fn update_child_filters() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // create the image with a child filter
    let child_filter = r"remove-me";
    let image_req =
        generators::gen_image(&group).child_filters(ChildFilters::default().mime(child_filter));
    client.images.create(&image_req).await?;
    // add a regular expression to it and remove the existing one
    let update = ImageUpdate::default().child_filters(
        ChildFiltersUpdate::default()
            .add_mime(r"new-filter")
            .remove_mime(child_filter),
    );
    client
        .images
        .update(&group, &image_req.name, &update)
        .await?;
    // get the updated image
    let image = client.images.get(&group, &image_req.name).await?;
    // make sure the update applied
    is!(image, update);
    Ok(())
}

#[tokio::test]
async fn update_bad_child_filters() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // try to add a bad regular expression to it
    let update = ImageUpdate::default()
        .child_filters(ChildFiltersUpdate::default().add_mime(r"unrecognized-escape\q"));
    let resp = client.images.update(&group, &image.name, &update).await;
    fail!(resp, 400, "filter regular expressions is invalid");
    // try to remove a regular expression it doesn't have
    let update = ImageUpdate::default()
        .child_filters(ChildFiltersUpdate::default().remove_mime(r"not-found"));
    let resp = client.images.update(&group, &image.name, &update).await;
    fail!(resp, 400, "missing one or more mime child filters");
    Ok(())
}

#[tokio::test]
async fn update_child_filters_files() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image with a file name and file extension child filter
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // swap out the file name and file extension filters this image was created with
    let update = ImageUpdate::default().child_filters(
        ChildFiltersUpdate::default()
            .add_file_name(r"report.*")
            .remove_file_name(r"note.*")
            .add_file_extensions(["dll", "so"])
            .remove_file_extension("exe"),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our updates were applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    Ok(())
}

#[tokio::test]
async fn update_args() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image with no args set
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // set every arg this image can have
    let update = ImageUpdate::default().args(
        ImageArgsUpdate::default()
            .entrypoint(vec!["/bin/bash", "-c"])
            .command(vec!["harvest", "--all"])
            .reaction("--reaction")
            .repo("--repo")
            .commit("--commit")
            .output(ArgStrategy::Kwarg("--output".to_owned()))
            .output_files_files(ArgStrategy::Append),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our updates were applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    // update just a single arg and make sure the rest are left alone
    let update = ImageUpdate::default().args(ImageArgsUpdate::default().reaction("--new-reaction"));
    client.images.update(&group, &image.name, &update).await?;
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.args.repo, Some("--repo".to_owned()));
    Ok(())
}

#[tokio::test]
async fn update_args_clear() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup an image with all of its args already set
    let image_req = generators::gen_full_image(&group);
    client.images.create(&image_req).await?;
    // clear every arg that can be cleared
    let update = ImageUpdate::default().args(
        ImageArgsUpdate::default()
            .clear_entrypoint()
            .clear_command()
            .clear_reaction()
            .clear_repo()
            .clear_commit(),
    );
    client
        .images
        .update(&group, &image_req.name, &update)
        .await?;
    // get the image and make sure every arg was cleared
    let updated = client.images.get(&group, &image_req.name).await?;
    is!(updated, update);
    is!(updated.args.entrypoint, Option::<Vec<String>>::None);
    is!(updated.args.command, Option::<Vec<String>>::None);
    is!(updated.args.reaction, Option::<String>::None);
    is!(updated.args.repo, Option::<String>::None);
    is!(updated.args.commit, Option::<String>::None);
    Ok(())
}

#[tokio::test]
async fn update_args_empty_clears() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup an image with all of its args already set
    let image_req = generators::gen_full_image(&group);
    client.images.create(&image_req).await?;
    // an empty entrypoint/command clears them instead of setting them
    let update = ImageUpdate::default().args(
        ImageArgsUpdate::default()
            .entrypoint(Vec::<String>::new())
            .command(Vec::<String>::new()),
    );
    client
        .images
        .update(&group, &image_req.name, &update)
        .await?;
    // get the image and make sure both args were cleared
    let updated = client.images.get(&group, &image_req.name).await?;
    is!(updated, update);
    is!(updated.args.entrypoint, Option::<Vec<String>>::None);
    is!(updated.args.command, Option::<Vec<String>>::None);
    Ok(())
}

#[tokio::test]
async fn update_modifiers() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image with no modifiers
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // set this images modifiers
    let update = ImageUpdate::default().modifiers("/data/modifiers");
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our update was applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.modifiers, Some("/data/modifiers".to_owned()));
    // an empty modifiers path clears it instead of setting it
    let update = ImageUpdate::default().modifiers("");
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our modifiers were cleared
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.modifiers, Option::<String>::None);
    Ok(())
}

#[tokio::test]
async fn update_image_bad() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // an image that is only whitespace is empty once it has been trimmed
    let update = ImageUpdate::default().image("   ");
    let resp = client.images.update(&group, &image.name, &update).await;
    fail!(resp, 400, "Image cannot be empty");
    Ok(())
}

#[tokio::test]
async fn update_env() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image seeded with an ENV_ARG and a REMOVE_ARG env var
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // add an env var with a value, an env var without one, and remove an existing one
    let update = ImageUpdate::default()
        .add_env("NEW_ARG", Some("new"))
        .add_env("FLAG", None::<&str>)
        .remove_env("REMOVE_ARG");
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our updates were applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.env.get("NEW_ARG"), Some(&Some("new".to_owned())));
    is!(updated.env.get("FLAG"), Some(&None::<String>));
    is!(updated.env.get("REMOVE_ARG"), None::<&Option<String>>);
    // the env vars we didn't touch should be left alone
    is!(updated.env.get("ENV_ARG"), Some(&Some("Test".to_owned())));
    Ok(())
}

#[tokio::test]
async fn update_clear_image() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // update the image with a new image path
    let update = ImageUpdate::default().image("rust:1.48.0");
    client.images.update(&group, &image.name, &update).await?;
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    // now clear the image path with a new ImageUpdate
    let update = ImageUpdate::default().clear_image();
    client.images.update(&group, &image.name, &update).await?;
    // get the image and ensure that the image path is empty
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.image, Option::<String>::None);
    Ok(())
}

#[tokio::test]
async fn update_clear_lifetime() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image with a job based lifetime
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // update the image with a new lifetime
    let update = ImageUpdate::default().lifetime(ImageLifetime::jobs(12));
    client.images.update(&group, &image.name, &update).await?;
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    // now clear the lifetime with a new ImageUpdate
    let update = ImageUpdate::default().clear_lifetime();
    client.images.update(&group, &image.name, &update).await?;
    // get the image and ensure that the lifetime is empty
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.lifetime, Option::<ImageLifetime>::None);
    Ok(())
}

#[tokio::test]
async fn update_resources_burstable() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // set the burstable resources this image can use
    let update = ImageUpdate::default().resources(
        ResourcesUpdate::default().burstable(
            BurstableResourcesUpdate::default()
                .cores(4.0)
                .memory("8Gi")?,
        ),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our updates were applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.resources.burstable.cpu, 4000);
    Ok(())
}

#[tokio::test]
async fn update_spawn_limit() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image with an unlimited spawn limit
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // cap the number of workers that can be spawned for this image
    let update = ImageUpdate::default().spawn_limit(SpawnLimits::Basic(5));
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our update was applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    let limit = unwrap_variant!(updated.spawn_limit, SpawnLimits::Basic);
    is!(limit, 5);
    // now lift that cap again
    let update = ImageUpdate::default().spawn_limit(SpawnLimits::Unlimited);
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our update was applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.spawn_limit, SpawnLimits::Unlimited);
    Ok(())
}

#[tokio::test]
async fn update_scaler() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image that is scaled in K8s
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // move this image over to the external scaler
    let update = ImageUpdate::default().scaler(ImageScaler::External);
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our update was applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.scaler, ImageScaler::External);
    Ok(())
}

#[tokio::test]
async fn update_scaler_kvm_bad() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // get a user client for a developer that cannot develop kvm images
    let user_client = generators::client(&client).await?;
    // Create a group
    let group = generators::groups(1, &user_client).await?.remove(0).name;
    // setup a random image that is scaled in K8s
    let image = generators::images(&group, 1, false, &user_client)
        .await?
        .remove(0);
    // this user cannot move an image over to a scaler they cannot develop for
    let update = ImageUpdate::default().scaler(ImageScaler::Kvm);
    let resp = user_client
        .images
        .update(&group, &image.name, &update)
        .await;
    fail!(resp, 401);
    Ok(())
}

#[tokio::test]
async fn update_security_context() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // set the user, group, and privilege escalation settings for this image
    let update = ImageUpdate::default().security_context(
        SecurityContextUpdate::default()
            .user(1000)
            .group(1001)
            .allow_escalation(),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our updates were applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.security_context.user, Some(1000));
    is!(updated.security_context.group, Some(1001));
    is!(updated.security_context.allow_privilege_escalation, true);
    // now clear the user and group and disallow privilege escalation
    let update = ImageUpdate::default().security_context(
        SecurityContextUpdate::default()
            .clear_user()
            .clear_group()
            .disallow_escalation(),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our updates were applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.security_context.user, Option::<i64>::None);
    is!(updated.security_context.group, Option::<i64>::None);
    is!(updated.security_context.allow_privilege_escalation, false);
    Ok(())
}

#[tokio::test]
async fn update_security_context_bad() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // get a user client
    let user_client = generators::client(&client).await?;
    // Create a group
    let group = generators::groups(1, &user_client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &user_client)
        .await?
        .remove(0);
    // only admins can update an images security context
    let update =
        ImageUpdate::default().security_context(SecurityContextUpdate::default().user(1000));
    let resp = user_client
        .images
        .update(&group, &image.name, &update)
        .await;
    fail!(resp, 401);
    Ok(())
}

#[tokio::test]
async fn update_dependencies_tags() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image with no tag dependencies
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // turn on tag dependencies for this image
    let update = ImageUpdate::default().dependencies(
        DependenciesUpdate::default().tags(
            TagDependencySettingsUpdate::default()
                .enable()
                .location("/test/tags")
                .kwarg("--tags")
                .strategy(DependencyPassStrategy::Paths),
        ),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our updates were applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.dependencies.tags.enabled, true);
    // now turn tag dependencies back off
    let update = ImageUpdate::default().dependencies(
        DependenciesUpdate::default().tags(TagDependencySettingsUpdate::default().disable()),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our update was applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.dependencies.tags.enabled, false);
    Ok(())
}

#[tokio::test]
async fn update_dependencies_children() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup an image that already depends on the children of the plant image
    let image_req = generators::gen_full_image(&group);
    client.images.create(&image_req).await?;
    // swap out the image we depend on the children of
    let update = ImageUpdate::default().dependencies(
        DependenciesUpdate::default().children(
            ChildrenDependencySettingsUpdate::default()
                .enable()
                .image("harvest")
                .remove_image("plant")
                .location("/updated/children")
                .kwarg("--new-children")
                .strategy(DependencyPassStrategy::Directory),
        ),
    );
    client
        .images
        .update(&group, &image_req.name, &update)
        .await?;
    // get the image and make sure our updates were applied
    let updated = client.images.get(&group, &image_req.name).await?;
    is!(updated, update);
    is_in!(updated.dependencies.children.images, "harvest".to_owned());
    Ok(())
}

#[tokio::test]
async fn update_dependencies_cache() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image with no cache dependencies
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // turn on cache dependencies for this image
    let update = ImageUpdate::default().dependencies(
        DependenciesUpdate::default().cache(
            CacheDependencySettingsUpdate::default()
                .enable()
                .location("/test/cache")
                .use_parent_cache()
                .generic(
                    GenericCacheDependencySettingsUpdate::default()
                        .kwarg("--cache")
                        .strategy(DependencyPassStrategy::Paths),
                ),
        ),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our updates were applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.dependencies.cache.enabled, true);
    is!(updated.dependencies.cache.use_parent_cache, true);
    // now stop using our parents cache and turn cache dependencies back off
    let update = ImageUpdate::default().dependencies(
        DependenciesUpdate::default().cache(
            CacheDependencySettingsUpdate::default()
                .disable()
                .ignore_parent_cache(),
        ),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our updates were applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.dependencies.cache.enabled, false);
    is!(updated.dependencies.cache.use_parent_cache, false);
    Ok(())
}

#[tokio::test]
async fn update_dependencies_clear_kwargs() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup an image that has a kwarg set for every dependency that supports one
    let image_req = generators::gen_full_image(&group);
    client.images.create(&image_req).await?;
    // clear every dependency kwarg this image has
    let update = ImageUpdate::default().dependencies(
        DependenciesUpdate::default()
            .samples(SampleDependencySettingsUpdate::default().clear_kwarg())
            .ephemeral(EphemeralDependencySettingsUpdate::default().clear_kwarg())
            .repos(RepoDependencySettingsUpdate::default().clear_kwarg())
            .tags(TagDependencySettingsUpdate::default().clear_kwarg())
            .children(ChildrenDependencySettingsUpdate::default().clear_kwarg())
            .cache(
                CacheDependencySettingsUpdate::default()
                    .generic(GenericCacheDependencySettingsUpdate::default().clear_kwarg()),
            ),
    );
    client
        .images
        .update(&group, &image_req.name, &update)
        .await?;
    // get the image and make sure every kwarg was cleared
    let updated = client.images.get(&group, &image_req.name).await?;
    is!(updated, update);
    is!(updated.dependencies.samples.kwarg, Option::<String>::None);
    is!(updated.dependencies.ephemeral.kwarg, Option::<String>::None);
    is!(updated.dependencies.repos.kwarg, Option::<String>::None);
    is!(updated.dependencies.tags.kwarg, Option::<String>::None);
    is!(updated.dependencies.children.kwarg, Option::<String>::None);
    is!(
        updated.dependencies.cache.generic.kwarg,
        Option::<String>::None
    );
    Ok(())
}

#[tokio::test]
async fn update_dependencies_naming() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // change how the sample dependencies for this image are named on disk
    let update =
        ImageUpdate::default().dependencies(DependenciesUpdate::default().samples(
            SampleDependencySettingsUpdate::default().naming(FileNamingStrategy::MostRecent),
        ));
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our update was applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(
        updated.dependencies.samples.naming,
        FileNamingStrategy::MostRecent
    );
    Ok(())
}

#[tokio::test]
async fn update_clean_up() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image that already has clean up settings
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // update every one of this images clean up settings
    let update = ImageUpdate::default().clean_up(
        CleanupUpdate::default()
            .script("/updated/script.py")
            .job_id(ArgStrategy::Kwarg("--new-job-id".to_owned()))
            .results(ArgStrategy::Append)
            .result_files_dir(ArgStrategy::Kwarg("--output-dir".to_owned())),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our updates were applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    let clean_up = updated
        .clean_up
        .ok_or_else(|| Error::new("Image has no clean up settings"))?;
    is!(clean_up.script, "/updated/script.py".to_owned());
    is!(clean_up.results, ArgStrategy::Append);
    Ok(())
}

#[tokio::test]
async fn update_clean_up_clear() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image that already has clean up settings
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // clear this images clean up settings entirely
    let update = ImageUpdate::default().clean_up(CleanupUpdate::default().clear());
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure its clean up settings were removed
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.clean_up.is_none(), true);
    Ok(())
}

#[tokio::test]
async fn update_clean_up_bad() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup an image with no clean up settings
    let image_req = generators::gen_ext_image(&group);
    client.images.create(&image_req).await?;
    // clean up settings can't be built without a script to clean up with
    let update = ImageUpdate::default()
        .clean_up(CleanupUpdate::default().job_id(ArgStrategy::Kwarg("--job-id".to_owned())));
    let resp = client.images.update(&group, &image_req.name, &update).await;
    fail!(resp, 400, "A clean up script must be set");
    Ok(())
}

#[tokio::test]
async fn update_kvm() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image with no kvm settings
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // kvm settings are created when both of the required settings are given
    let update = ImageUpdate::default().kvm(
        KvmUpdate::default()
            .xml("/kvm/golden.xml")
            .qcow2("/kvm/golden.qcow2"),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our kvm settings were created
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    // now that this image has kvm settings we can update just one of them
    let update = ImageUpdate::default().kvm(KvmUpdate::default().xml("/kvm/updated.xml"));
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure only the xml path changed
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    let kvm = updated
        .kvm
        .ok_or_else(|| Error::new("Image has no kvm settings"))?;
    is!(kvm.xml, "/kvm/updated.xml".to_owned());
    is!(kvm.qcow2, "/kvm/golden.qcow2".to_owned());
    Ok(())
}

#[tokio::test]
async fn update_kvm_bad() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image with no kvm settings
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // kvm settings cannot be created from just one of the two required settings
    let update = ImageUpdate::default().kvm(KvmUpdate::default().xml("/kvm/golden.xml"));
    let resp = client.images.update(&group, &image.name, &update).await;
    fail!(resp, 400, "xml and qcow2 must both be set");
    Ok(())
}

#[tokio::test]
async fn update_output_collection() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // update where children are collected from and restrict who can see our results
    let update = ImageUpdate::default().output_collection(
        OutputCollectionUpdate::default()
            .children("/updated/children")
            .as_filesystem(true)
            .group(&group),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our updates were applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.output_collection.as_filesystem, true);
    is_in!(updated.output_collection.groups, group);
    // now lift our group restrictions
    let update =
        ImageUpdate::default().output_collection(OutputCollectionUpdate::default().clear_groups());
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our group restrictions were cleared
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.output_collection.groups.is_empty(), true);
    Ok(())
}

#[tokio::test]
async fn update_output_collection_files() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image with a seeded files handler
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // update the files handlers paths and wipe the result files it collects
    let update = ImageUpdate::default().output_collection(
        OutputCollectionUpdate::default().files(
            FilesHandlerUpdate::default()
                .results("/updated/results")
                .result_files("/updated/result_files")
                .tags("/updated/tags")
                .add_name("corn.csv")
                .clear_names(),
        ),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our updates were applied
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.output_collection.files.names.is_empty(), true);
    is!(
        updated.output_collection.files.tags,
        "/updated/tags".to_owned()
    );
    // now reset the entire files handler back to its defaults
    let update =
        ImageUpdate::default().output_collection(OutputCollectionUpdate::default().clear_files());
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure the files handler was reset
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    Ok(())
}

#[tokio::test]
async fn update_output_collection_auto_tag() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image with no auto tag settings
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // an update for an auto tag setting that doesn't exist yet creates it
    let update = ImageUpdate::default().output_collection(
        OutputCollectionUpdate::default().auto_tag(
            "Plant",
            AutoTagUpdate::default()
                .logic(AutoTagLogic::Equal(serde_json::json!("Corn")))
                .key("plant".to_owned()),
        ),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure our auto tag setting was created
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    let auto_tag = updated
        .output_collection
        .auto_tag
        .get("Plant")
        .ok_or_else(|| Error::new("Image has no Plant auto tag settings"))?;
    is!(auto_tag.key, Some("plant".to_owned()));
    // now clear the key we look this tags value up under
    let update = ImageUpdate::default().output_collection(
        OutputCollectionUpdate::default().auto_tag("Plant", AutoTagUpdate::default().clear_key()),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure the key was cleared
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    let auto_tag = updated
        .output_collection
        .auto_tag
        .get("Plant")
        .ok_or_else(|| Error::new("Image has no Plant auto tag settings"))?;
    is!(auto_tag.key, Option::<String>::None);
    // now delete this auto tag setting entirely
    let update = ImageUpdate::default().output_collection(
        OutputCollectionUpdate::default().auto_tag("Plant", AutoTagUpdate::default().delete()),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure the auto tag setting was deleted
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(
        updated.output_collection.auto_tag.contains_key("Plant"),
        false
    );
    Ok(())
}

#[tokio::test]
async fn update_add_volume_bad_name() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // volume names cannot be longer then 25 characters
    let update = ImageUpdate::default().add_volume(Volume::new(
        "this-volume-name-is-way-too-long",
        "/files",
        VolumeTypes::ConfigMap,
    ));
    let resp = client.images.update(&group, &image.name, &update).await;
    fail!(resp, 400, "volume name must be between");
    // volume names must be lowercase alphanumeric or a '-'
    let update = ImageUpdate::default().add_volume(Volume::new(
        "Test-Vol",
        "/files",
        VolumeTypes::ConfigMap,
    ));
    let resp = client.images.update(&group, &image.name, &update).await;
    fail!(resp, 400, "volume name must be only lowercase alphanumeric");
    Ok(())
}

#[tokio::test]
async fn update_remove_volume_missing() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image with a single volume
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // removing a volume this image doesn't have is a no op
    let update = ImageUpdate::default().remove_volume("not-a-volume");
    let resp = client.images.update(&group, &image.name, &update).await?;
    is!(resp.status().as_u16(), 204);
    // get the image and make sure the volumes it does have were left alone
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    is!(updated.volumes.len(), image.volumes.len());
    Ok(())
}

#[tokio::test]
async fn update_ban_readd_bad() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // ban this image
    let update = ImageUpdate::default()
        .bans(ImageBanUpdate::default().add_ban(ImageBan::new(ImageBanKind::generic("Test ban!"))));
    client.images.update(&group, &image.name, &update).await?;
    // bans can only be added or removed so re-adding the same ban is an error
    let resp = client.images.update(&group, &image.name, &update).await;
    fail!(resp, 400, "already exists");
    Ok(())
}

#[tokio::test]
async fn update_runtimes() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // register the node our test worker will run on
    generators::node(
        "runtimes-cluster",
        "runtimes-node",
        Resources::default(),
        &client,
    )
    .await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // create a pipeline with a single stage so we only have one image to run
    let pipe_req = generators::gen_pipe(&group, 1, false, &client).await?;
    client.pipelines.create(&pipe_req).await?;
    // get the pipeline for this pipeline order
    let pipe = client.pipelines.get(&group, &pipe_req.name).await?;
    // get the only stage in this pipeline
    let stage = pipe
        .order
        .iter()
        .flatten()
        .next()
        .ok_or_else(|| Error::new("Generated pipeline has no stages"))?;
    // this images average runtime should still be the default
    let image = client.images.get(&group, stage).await?;
    is!(image.runtime, 600.0);
    // create a reaction so we have a job to claim
    let req = generators::gen_reaction(&group, &pipe, None);
    client.reactions.create(&req).await?;
    // register the worker that will claim this job
    generators::worker(
        "runtimes-cluster",
        "runtimes-node",
        "runtimes",
        &group,
        &pipe.name,
        stage,
        &client,
    )
    .await?;
    // claim the job for this stage
    let job = client
        .jobs
        .claim(
            &group,
            &pipe.name,
            stage,
            "runtimes-cluster",
            "runtimes-node",
            "runtimes",
            1,
        )
        .await?;
    // complete this job with a runtime that is nowhere near the default
    let runtime = 42;
    let logs = generators::stage_logs();
    client.jobs.proceed(&job[0], &logs, runtime).await?;
    // recalculate the average runtimes for every image
    let resp = client.images.update_runtimes().await?;
    is!(resp.status().as_u16(), 204);
    // this images average runtime should now be the runtime of its only completed job
    let image = client.images.get(&group, stage).await?;
    is!(image.runtime, 42.0);
    // clean up the worker we registered
    generators::delete_worker("runtimes", &client).await?;
    Ok(())
}

#[tokio::test]
async fn update_runtimes_bad() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // get a user client
    let user_client = generators::client(&client).await?;
    // only admins can recalculate the average runtimes for all images
    let resp = user_client.images.update_runtimes().await;
    fail!(resp, 401);
    Ok(())
}

#[tokio::test]
async fn update_bans() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // update the image with bans
    let mut bans = vec![
        ImageBan::new(ImageBanKind::generic("Test ban 1!")),
        ImageBan::new(ImageBanKind::image_url(
            image.image.clone().unwrap_or_default(),
        )),
    ];
    let update = ImageUpdate::default().bans(ImageBanUpdate::default().add_bans(bans.clone()));
    client.images.update(&group, &image.name, &update).await?;
    // successfully remove a ban from the image
    let update = ImageUpdate::default().bans(
        ImageBanUpdate::default().remove_ban(
            bans.pop()
                .ok_or_else(|| Error::new("Popped empty bans vec"))?
                .id,
        ),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure the ban was removed
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    // attempt to remove a ban as a non admin
    let user_client = generators::client(&client).await?;
    let username = user_client.users.info().await?.username;
    // add the user to the group
    let group_update =
        GroupUpdate::default().users(GroupUsersUpdate::default().direct_add(username));
    client.groups.update(&group, &group_update).await?;
    let update = ImageUpdate::default().bans(
        ImageBanUpdate::default().remove_ban(
            bans.pop()
                .ok_or_else(|| Error::new("Popped empty bans vec"))?
                .id,
        ),
    );
    let resp = user_client
        .images
        .update(&group, &image.name, &update)
        .await;
    fail!(resp, 401);
    // remove the second ban
    client.images.update(&group, &image.name, &update).await?;
    // get the image and make sure the ban was removed
    let updated = client.images.get(&group, &image.name).await?;
    is!(updated, update);
    // attempt to remove a ban that doesn't exist
    let update = ImageUpdate::default().bans(ImageBanUpdate::default().remove_ban(Uuid::new_v4()));
    let resp = client.images.update(&group, &image.name, &update).await;
    fail!(resp, 404);
    Ok(())
}

#[serial_test::serial]
#[tokio::test]
async fn update_fix_ban() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // get a user client
    let user_client = generators::client(&client).await?;
    // Create a group
    let group = generators::groups(1, &user_client).await?.remove(0).name;
    // reset settings
    client
        .system
        .reset_settings(&SystemSettingsResetParams::default().no_scan())
        .await?;
    // allow all host paths
    let settings_update = SystemSettingsUpdate::default().allow_unrestricted_host_paths(true);
    client
        .system
        .update_settings(
            &settings_update,
            &SystemSettingsUpdateParams::default().no_scan(),
        )
        .await?;
    // create an image with a host path
    let vol_path = "/some/path";
    let image_req = generators::gen_host_path(&group, vol_path);
    client.images.create(&image_req).await?;
    // get the host path volume
    let host_path_volume = image_req
        .volumes
        .iter()
        .find(|vol| match vol.archetype {
            VolumeTypes::HostPath => vol.host_path.is_some(),
            _ => false,
        })
        .unwrap();
    // set a host path ban on that image
    let image_update = ImageUpdate::default().bans(ImageBanUpdate::default().add_ban(
        ImageBan::new(ImageBanKind::host_path(&host_path_volume.name, vol_path)),
    ));
    client
        .images
        .update(&group, &image_req.name, &image_update)
        .await?;
    // verify that the ban is set
    let image = user_client.images.get(&group, &image_req.name).await?;
    is!(image.bans.len(), 1, "Set ban");
    let ban = image.bans.values().next().unwrap();
    let ban = unwrap_variant!(&ban.ban_kind, ImageBanKind::InvalidHostPath);
    is!(
        PathBuf::from(vol_path),
        ban.host_path,
        "Banned host path is correct"
    );
    // remove the problematic volume
    let image_update = ImageUpdate::default().remove_volume(&ban.volume_name);
    user_client
        .images
        .update(&group, &image.name, &image_update)
        .await?;
    // verify that the ban was removed
    let image = user_client.images.get(&group, &image_req.name).await?;
    is!(image.bans.len(), 0, "Ban removed after bad volume removed");
    // reset settings
    client
        .system
        .reset_settings(&SystemSettingsResetParams::default().no_scan())
        .await?;
    Ok(())
}

#[tokio::test]
async fn update_network_policy() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // create groups
    let groups = generators::groups(2, &client)
        .await?
        .into_iter()
        .map(|g| g.name)
        .collect::<Vec<String>>();
    // create network policies in those groups
    let network_policies = generators::network_policies(&groups, 2, &client).await?;
    // create an image with those network policies that's in one of the policies' groups
    let mut image_req = generators::gen_image(&groups[0]);
    image_req = image_req.network_policies(network_policies.iter().map(|p| &p.name));
    client.images.create(&image_req).await?;
    // remove one of the network policies
    let update = ImageUpdate::default().network_policies(
        ImageNetworkPolicyUpdate::default().remove_policy(&network_policies[0].name),
    );
    client
        .images
        .update(&image_req.group, &image_req.name, &update)
        .await?;
    let image = client.images.get(&image_req.group, &image_req.name).await?;
    is!(
        image.network_policies.len(),
        1,
        "network policies length is 1 after remove policy"
    );
    is!(
        image.network_policies.iter().next().unwrap(),
        &network_policies[1].name,
        "policy is policy 1 after add/remove same policy"
    );
    // remove the one network policy and add the other one
    let update = ImageUpdate::default().network_policies(
        ImageNetworkPolicyUpdate::default()
            .add_policy(&network_policies[0].name)
            .remove_policy(&network_policies[1].name),
    );
    client
        .images
        .update(&image_req.group, &image_req.name, &update)
        .await?;
    let image = client.images.get(&image_req.group, &image_req.name).await?;
    is!(
        image.network_policies.len(),
        1,
        "network policies length is 1 after add/remove policies"
    );
    is!(
        image.network_policies.iter().next().unwrap(),
        &network_policies[0].name,
        "policy is policy 0 after add/remove same policy"
    );
    Ok(())
}

#[tokio::test]
async fn update_bad_network_policy() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // create groups
    let image_group = generators::groups(1, &client).await?.remove(0).name;
    let policy_group = generators::groups(1, &client).await?.remove(0).name;
    // create image with a policy
    let image_policy = generators::network_policies(&[image_group.clone()], 1, &client)
        .await?
        .remove(0);
    let mut image_req = generators::gen_image(&image_group);
    image_req = image_req.network_policy(&image_policy.name);
    client.images.create(&image_req).await?;
    // create network policy
    let not_image_policy = generators::network_policies(&[policy_group.clone()], 1, &client)
        .await?
        .remove(0);
    // attempt to add a network policy that is not in the image's group
    let update = ImageUpdate::default()
        .network_policies(ImageNetworkPolicyUpdate::default().add_policy(&not_image_policy.name));
    let resp = client
        .images
        .update(&image_req.group, &image_req.name, &update)
        .await;
    fail!(resp, 404);
    // attempt to remove a network policy that the image does not have
    let update = ImageUpdate::default()
        .network_policies(ImageNetworkPolicyUpdate::default().remove_policy("policy-no-exist"));
    let resp = client
        .images
        .update(&image_req.group, &image_req.name, &update)
        .await;
    fail!(resp, 400);
    // attempt to add a network policy the image already has
    let update = ImageUpdate::default()
        .network_policies(ImageNetworkPolicyUpdate::default().add_policy(&image_policy.name));
    let resp = client
        .images
        .update(&image_req.group, &image_req.name, &update)
        .await;
    fail!(resp, 400);
    // attempt to add a network policy that does not exist at all
    let update = ImageUpdate::default()
        .network_policies(ImageNetworkPolicyUpdate::default().add_policy("policy-no-exist"));
    let resp = client
        .images
        .update(&image_req.group, &image_req.name, &update)
        .await;
    fail!(resp, 404);
    // try to change the image's scaler type while it has a network policy
    let update = ImageUpdate::default().scaler(ImageScaler::External);
    let resp = client
        .images
        .update(&image_req.group, &image_req.name, &update)
        .await;
    fail!(resp, 400, "non-K8's while network policies are applied");
    // create a new image not scaled by K8's
    let image_req = generators::gen_ext_image(&image_group);
    client.images.create(&image_req).await?;
    // try to add a network policy to an image not scaled in K8's
    let update = ImageUpdate::default()
        .network_policies(ImageNetworkPolicyUpdate::default().add_policy(&image_policy.name));
    let resp = client
        .images
        .update(&image_req.group, &image_req.name, &update)
        .await;
    fail!(resp, 400, "only be applied to images scaled in K8s");
    Ok(())
}

#[tokio::test]
async fn notifications_bans() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // get user client
    let user_client = generators::client(&client).await?;
    // Create a group
    let group = generators::groups(1, &user_client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &user_client)
        .await?
        .remove(0);
    // update the image with bans
    let generic_ban_msg = "Test ban 1!";
    let mut bans = vec![
        ImageBan::new(ImageBanKind::generic(generic_ban_msg)),
        ImageBan::new(ImageBanKind::image_url(
            image.image.clone().unwrap_or_default(),
        )),
    ];
    let update = ImageUpdate::default().bans(ImageBanUpdate::default().add_bans(bans.clone()));
    client.images.update(&group, &image.name, &update).await?;
    // get the images notifications and make sure there was a notification added for the ban
    let notifications = user_client
        .images
        .get_notifications(&group, &image.name)
        .await?;
    is!(notifications.len(), 2);
    let mut ban_ids: Vec<Uuid> = bans.iter().map(|ban| ban.id).collect();
    contains!(ban_ids, notifications[0].ban_id.as_ref().unwrap());
    contains!(ban_ids, notifications[1].ban_id.as_ref().unwrap());
    // successfully remove a ban from the image
    let update = ImageUpdate::default().bans(
        ImageBanUpdate::default().remove_ban(
            bans.pop()
                .ok_or_else(|| Error::new("Popped empty bans vec"))?
                .id,
        ),
    );
    client.images.update(&group, &image.name, &update).await?;
    // get the images notifications and make sure that notification was automatically removed
    let notifications = user_client
        .images
        .get_notifications(&group, &image.name)
        .await?;
    is!(notifications.len(), 1);
    ban_ids.pop();
    contains!(ban_ids, notifications[0].ban_id.as_ref().unwrap());
    is!(notifications[0].msg, generic_ban_msg);
    Ok(())
}

#[tokio::test]
async fn create_notification() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // create an image notification
    let req = NotificationRequest::new("Test warning message!", NotificationLevel::Warn);
    client
        .images
        .create_notification(&group, &image.name, &req, &NotificationParams::default())
        .await?;
    // make sure the image notification is there
    let notifications = client.images.get_notifications(&group, &image.name).await?;
    is!(notifications.len(), 1);
    is!(notifications[0], req);
    is!(notifications[0].key.group, group);
    is!(notifications[0].key.image, image.name);
    Ok(())
}

#[tokio::test]
async fn create_notification_bad() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // get user client
    let user_client = generators::client(&client).await?;
    // Create a group
    let group = generators::groups(1, &user_client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &user_client)
        .await?
        .remove(0);
    // fail to create an image notification as a regular user
    let req = NotificationRequest::new("Test warning message!", NotificationLevel::Warn);
    let resp = user_client
        .images
        .create_notification(&group, &image.name, &req, &NotificationParams::default())
        .await;
    fail!(resp, 401);
    // fail to create an image notification for an image that doesn't exist
    let resp = client
        .images
        .create_notification(
            &group,
            "does-not-exist",
            &req,
            &NotificationParams::default(),
        )
        .await;
    fail!(resp, 404);
    Ok(())
}

#[tokio::test]
async fn delete_notification() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &client)
        .await?
        .remove(0);
    // create a notification
    let req = NotificationRequest::new("Test warning message!", NotificationLevel::Warn);
    client
        .images
        .create_notification(&group, &image.name, &req, &NotificationParams::default())
        .await?;
    // get the image notification
    let notification = client
        .images
        .get_notifications(&group, &image.name)
        .await?
        .remove(0);
    // delete the notification
    client
        .images
        .delete_notification(&group, &image.name, &notification.id)
        .await?;
    // check that the notification was deleted
    let notifications = client.images.get_notifications(&group, &image.name).await?;
    is!(notifications.len(), 0);
    Ok(())
}

#[tokio::test]
async fn delete_notification_bad() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // get user client
    let user_client = generators::client(&client).await?;
    // Create a group
    let group = generators::groups(1, &user_client).await?.remove(0).name;
    // setup a random image
    let image = generators::images(&group, 1, false, &user_client)
        .await?
        .remove(0);
    // create a notification
    let req = NotificationRequest::new("Test warning message!", NotificationLevel::Warn);
    client
        .images
        .create_notification(&group, &image.name, &req, &NotificationParams::default())
        .await?;
    // get the image's notifications
    let notification = user_client
        .images
        .get_notifications(&group, &image.name)
        .await?
        .remove(0);
    // fail to delete an image notification as a regular user
    let resp = user_client
        .images
        .delete_notification(&group, &image.name, &notification.id)
        .await;
    fail!(resp, 401);
    // fail to delete an image notification for an image that doesn't exist
    let resp = client
        .images
        .delete_notification(&group, "no-exists", &notification.id)
        .await;
    fail!(resp, 404);
    // fail to delete an image notification that doesn't exist
    let resp = client
        .images
        .delete_notification(&group, &image.name, &Uuid::new_v4())
        .await;
    fail!(resp, 404);
    Ok(())
}

#[cfg(all(feature = "sync", not(feature = "python")))]
#[test]
fn create_notification_blocking() -> Result<(), Error> {
    // get admin client
    let client = test_utilities::admin_client_blocking()?;
    // Create a group
    let group = generators::groups_blocking(1, &client)?.remove(0).name;
    // setup a random image
    let image = generators::images_blocking(&group, 1, false, &client)?.remove(0);
    // create an image notification
    let req = NotificationRequest::new("Test warning message!", NotificationLevel::Warn);
    client
        .images
        .create_notification(&group, &image.name, &req, &NotificationParams::default())?;
    // make sure the image notification is there
    let notifications = client.images.get_notifications(&group, &image.name)?;
    is!(notifications.len(), 1);
    is!(notifications[0], req);
    is!(notifications[0].key.group, group);
    is!(notifications[0].key.image, image.name);
    Ok(())
}
