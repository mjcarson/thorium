use crate::models::Image;
use crate::utils::Shared;

/// The keys to use to access images data/sets
#[derive(Debug)]
pub struct ImageKeys {
    // The key to store/retrieve image data at
    pub data: String,
    // The key to images in this group
    pub set: String,
    // The key to the time to complete queue for this image
    #[allow(dead_code)]
    pub ttc: String,
}

impl ImageKeys {
    /// Builds the keys to access image data/sets in redis
    ///
    /// # Arguments
    ///
    /// * `request` - Image request object
    /// * `shared` - Shared Thorium objects
    pub fn new(request: &Image, shared: &Shared) -> Self {
        // build key to store  image data at
        let data = Self::data(&request.group, &request.name, shared);
        // build key groups image set
        let set = Self::set(&request.group, shared);
        // build key to the ttc queue for this image
        let ttc = Self::ttc_queue(&request.group, &request.name, shared);
        // build key object
        ImageKeys { data, set, ttc }
    }

    /// Builds keys to Image data
    ///
    /// # Arguments
    ///
    /// * `group` - The group this image is in
    /// * `name` - The name of the image
    /// * `shared` - Shared Thorium objects
    pub fn data(group: &str, name: &str, shared: &Shared) -> String {
        format!(
            "{ns}:image_data:{group}:{name}",
            ns = shared.config.thorium.namespace,
            group = group,
            name = name
        )
    }

    /// Builds key to group images set
    ///
    /// # Arguments
    ///
    /// * `group` - The group to get images from
    /// * `shared` - Shared Thorium objects
    pub fn set(group: &str, shared: &Shared) -> String {
        format!(
            "{ns}:images:{group}",
            ns = shared.config.thorium.namespace,
            group = group
        )
    }

    /// Builds key to the time to complete queue for an image
    ///
    /// # Arguments
    ///
    /// * `group` - The goup this image is apart of
    /// * `image` - The name of this image
    /// * `shared` - Shared Thorium objects
    pub fn ttc_queue(group: &str, image: &str, shared: &Shared) -> String {
        format!(
            "{ns}:image_ttc:{group}:{image}",
            ns = shared.config.thorium.namespace,
            group = group,
            image = image,
        )
    }

    /// Builds key to set of pipelines using this image
    ///
    /// # Arguments
    ///
    /// * `group` - The goup this image is apart of
    /// * `image` - The name of this image
    /// * `shared` - Shared Thorium objects
    pub fn used_by(group: &str, image: &str, shared: &Shared) -> String {
        format!(
            "{ns}:image_used_by:{group}:{image}",
            ns = shared.config.thorium.namespace,
            group = group,
            image = image,
        )
    }
}
