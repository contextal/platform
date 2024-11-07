//! Prometheus utility constants and functions
const JOB_RECEIVED: &str = "frontend_jobs_received_total";
const JOB_TIMEDOUT: &str = "frontend_jobs_timed_out_total";
const JOB_MAXRETRIES: &str = "frontend_jobs_maxretries_total";
const JOB_COMPLETED: &str = "frontend_jobs_completed_total";
const JOB_RESCHEDULED: &str = "frontend_jobs_rescheduled_total";
const JOBS_WAITING: &str = "frontend_job_waiting_count";
const OBJ_COMPLETED_TIME: &str = "frontend_object_processing_time_seconds";
const WORK_PROC_TIME: &str = "frontend_work_processing_time_seconds";

pub fn init_metrics() {
    metrics::describe_counter!(JOB_RECEIVED, "Total number of received jobs");
    metrics::describe_counter!(JOB_TIMEDOUT, "Total number of expired jobs");
    metrics::describe_counter!(JOB_MAXRETRIES, "Total number of jobs exceeding max retries");
    metrics::describe_counter!(JOB_COMPLETED, "Total number of completed jobs");
    metrics::describe_counter!(JOB_RESCHEDULED, "Total number of rescheduled jobs");
    metrics::describe_gauge!(
        JOBS_WAITING,
        "Number of jobs currently awaiting child results"
    );
    metrics::describe_histogram!(
        OBJ_COMPLETED_TIME,
        metrics::Unit::Seconds,
        "Time to process an object (excluding children)"
    );
    metrics::describe_histogram!(
        WORK_PROC_TIME,
        metrics::Unit::Seconds,
        "Time to fully process a work request"
    );
}

pub fn job_received() {
    metrics::counter!(JOB_RECEIVED).increment(1);
}

pub fn job_timed_out() {
    metrics::counter!(JOB_TIMEDOUT).increment(1);
}

pub fn job_max_retries() {
    metrics::counter!(JOB_MAXRETRIES).increment(1);
}

pub fn job_completed() {
    metrics::counter!(JOB_COMPLETED).increment(1);
}

pub fn job_rescheduled() {
    metrics::counter!(JOB_RESCHEDULED).increment(1);
}

pub fn set_waiting_count(count: usize) {
    metrics::gauge!(JOBS_WAITING).set(count as f64);
}

pub fn set_job_processing_time(elapsed: &std::time::Duration) {
    metrics::histogram!(OBJ_COMPLETED_TIME).record(elapsed.as_secs_f64());
}

pub fn set_work_processing_time(elapsed: std::time::Duration) {
    metrics::histogram!(WORK_PROC_TIME).record(elapsed.as_secs_f64());
}
