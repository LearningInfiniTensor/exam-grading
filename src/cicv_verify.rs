use crate::{run::run, app_state::AppState};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tokio::task;
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize)]
struct ExerciseResult {
    name: String,
    result: bool,
}

#[derive(Serialize, Deserialize)]
struct CheckResult {
    exercises: Vec<ExerciseResult>,
    user_name: Option<String>,
    statistics: Statistics,
}

#[derive(Serialize, Deserialize)]
struct Statistics {
    total_exercations: usize,
    total_succeeds: usize,
    total_failures: usize,
    total_time: u64,
}

pub async fn cicv_verify(app_state: &mut AppState) -> Result<()> {
    // 并行验证所有练习
    let exercises = app_state.exercises().to_vec();
    let mut handles = vec![];
    for exercise in &exercises {
        let exercise_name = exercise.name.clone();
        let mut app_state = app_state.clone();
        let handle = task::spawn(async move {
            let start = Instant::now();
            app_state.set_current_exercise_by_name(&exercise_name).unwrap();
            let result = run(&mut app_state).context(format!("Failed to run {}", exercise_name));
            let duration = start.elapsed();
            (exercise_name, result, duration)
        });
        handles.push(handle);
    }

    // 收集所有结果
    let mut results = vec![];
    for handle in handles {
        let result = handle.await.context("Failed to join task")?;
        results.push(result);
    }

    // 生成验证结果输出
    let mut exercise_results = vec![];
    let mut total_succeeds = 0;
    let mut total_failures = 0;
    let mut total_time = 0;

    for (name, result, duration) in results {
        let passed = result.is_ok();
        if passed {
            total_succeeds += 1;
        } else {
            total_failures += 1;
        }
        total_time += duration.as_secs();
        exercise_results.push(ExerciseResult {
            name: name.to_string(),
            result: passed,
        });
    }

    let check_result = CheckResult {
        exercises: exercise_results,
        user_name: None,
        statistics: Statistics {
            total_exercations: exercises.len(),
            total_succeeds,
            total_failures,
            total_time,
        },
    };

    // 将结果写入文件
    let json_result = serde_json::to_string_pretty(&check_result)?;
    let result_path = Path::new(".github/result/rust_result.json");
    fs::create_dir_all(result_path.parent().unwrap())?;
    fs::write(result_path, json_result)?;

    Ok(())
}
