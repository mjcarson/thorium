use uuid::Uuid;

use crate::models::jobs::{JobStatus, RawJob};
use crate::utils::Shared;

/// The keys to store/retrieve job data/queues
#[derive(Debug)]
pub struct JobKeys {
    /// The priority queue the job is in
    pub status: String,
    /// The job data
    pub data: String,
}

impl JobKeys {
    /// Builds the keys to access job queues/data in redis
    ///
    /// # Arguments
    ///
    /// * `job` - Job object to build keys for
    /// * `shared` - Shared Thorium objects
    pub fn new(job: &RawJob, shared: &Shared) -> Self {
        // base key to job queue
        let status = Self::status_queue(
            &job.group,
            &job.pipeline,
            &job.stage,
            &job.creator,
            &job.status,
            shared,
        );
        // build key to store job data at
        let data = Self::data(&job.id, shared);
        // build key object
        JobKeys { status, data }
    }

    /// Builds key to status queue
    ///
    /// # Arguments
    ///
    /// * `group` - The group the job is in
    /// * `pipeline` - The pipeline the job is for
    /// * `stage` - The stage of the pipeline the job is in
    /// * `user` - The user that is requesting this job
    /// * `status` - The status for this job
    /// * `shared` - Shared Thorium objects
    pub fn status_queue(
        group: &str,
        pipeline: &str,
        stage: &str,
        user: &str,
        status: &JobStatus,
        shared: &Shared,
    ) -> String {
        // base key to build the queue key off of
        format!(
            "{ns}:job_queue:{group}:{pipeline}:{stage}:{user}:{status}",
            ns = shared.config.thorium.namespace,
            group = group,
            pipeline = pipeline,
            stage = stage,
            user = user,
            status = status
        )
    }

    /// Builds key to job data
    ///
    /// # Arguments
    ///
    /// * `id` - The uuidv4 of the job
    /// * `shared` - Shared Thorium objects
    pub fn data(id: &Uuid, shared: &Shared) -> String {
        format!(
            "{ns}:job_data:{id}",
            ns = shared.config.thorium.namespace,
            id = id
        )
    }

    /// Builds key to job data
    ///
    /// # Arguments
    ///
    /// * `id` - The uuidv4 of the job as a str
    /// * `shared` - Shared Thorium objects
    pub fn data_str(id: &Uuid, shared: &Shared) -> String {
        format!(
            "{ns}:job_data:{id}",
            ns = shared.config.thorium.namespace,
            id = id
        )
    }
}
