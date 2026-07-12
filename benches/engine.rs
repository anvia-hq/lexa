use criterion::{black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use lexa::engine::{Engine, SearchOptions};
use lexa::freshness;
use lexa::snapshot;
use lexa::store::Op;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

const CACHE_CAPACITY: u32 = 16_384;

fn rust_fixture(index: usize) -> (String, String) {
    let dependency = if index == 0 {
        String::new()
    } else {
        format!("use crate::module_{}::Service{};\n", index - 1, index - 1)
    };
    let call_dependency = if index == 0 {
        "self.value".to_string()
    } else {
        format!(
            "Service{}::new(self.value).handle_request(input)",
            index - 1
        )
    };

    let path = format!("src/module_{index}.rs");
    let content = format!(
        "{dependency}\
pub struct Service{index} {{
    value: usize,
}}

impl Service{index} {{
    pub fn new(value: usize) -> Self {{
        Self {{ value }}
    }}

    pub fn handle_request(&self, input: &str) -> usize {{
        let normalized = input.trim().to_lowercase();
        if normalized.contains(\"needle_token_{index}\") {{
            return self.value + normalized.len();
        }}
        {call_dependency}
    }}
}}

pub fn route_{index}(input: &str) -> usize {{
    let service = Service{index}::new({index});
    service.handle_request(input)
}}
"
    );

    (path, content)
}

fn write_project(file_count: usize) -> TempDir {
    let dir = TempDir::new().expect("create temp benchmark project");
    fs::create_dir_all(dir.path().join("src")).expect("create src directory");

    for index in 0..file_count {
        let (path, content) = rust_fixture(index);
        let full_path = dir.path().join(path);
        fs::write(full_path, content).expect("write benchmark fixture");
    }

    dir
}

fn build_engine_from_project(root: &Path) -> Engine {
    let mut engine = Engine::new(CACHE_CAPACITY);
    let count = engine.index_project(root);
    assert!(count > 0);
    engine
}

fn edited_fixture(index: usize) -> String {
    let (_, mut content) = rust_fixture(index);
    content.push_str(
        "\npub fn benchmark_added_symbol(input: &str) -> usize {\n    route_0(input) + input.len()\n}\n",
    );
    content
}

fn bench_project_indexing(c: &mut Criterion) {
    let mut group = c.benchmark_group("project_index");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));

    for file_count in [100usize, 500] {
        let project = write_project(file_count);
        group.bench_with_input(
            BenchmarkId::from_parameter(file_count),
            &file_count,
            |b, _| {
                b.iter(|| {
                    let mut engine = Engine::new(CACHE_CAPACITY);
                    let count = engine.index_project(black_box(project.path()));
                    black_box((
                        count,
                        engine.symbol_index_count(),
                        engine.word_index_count(),
                    ));
                });
            },
        );
    }

    group.finish();
}

fn bench_search(c: &mut Criterion) {
    let project = write_project(1_000);
    let engine = build_engine_from_project(project.path());

    let mut group = c.benchmark_group("search");
    group.sample_size(20);

    group.bench_function("exact_word", |b| {
        b.iter(|| black_box(engine.search(black_box("handle_request"), black_box(50))));
    });

    group.bench_function("unique_token", |b| {
        b.iter(|| black_box(engine.search(black_box("needle_token_777"), black_box(20))));
    });

    group.bench_function("regex", |b| {
        b.iter(|| {
            black_box(
                engine
                    .search_regex(black_box(r"Service\d+::new"), black_box(50))
                    .expect("valid regex"),
            )
        });
    });

    let scoped_options = SearchOptions {
        max_results: 50,
        regex: false,
        scope: true,
        compact: false,
        paths_only: false,
        path_glob: Some("src/*.rs".to_string()),
    };
    group.bench_function("rich_scoped", |b| {
        b.iter(|| {
            black_box(
                engine
                    .search_rich(black_box("handle_request"), black_box(&scoped_options))
                    .expect("search succeeds"),
            )
        });
    });

    group.bench_function("symbol_defs", |b| {
        b.iter(|| black_box(engine.find_symbol(black_box("Service777"))));
    });

    group.bench_function("callers", |b| {
        b.iter(|| black_box(engine.find_callers(black_box("handle_request"), black_box(50))));
    });

    group.finish();
}

fn bench_incremental_edit(c: &mut Criterion) {
    let project = write_project(500);
    let edited_content = edited_fixture(250);

    let mut group = c.benchmark_group("incremental_edit");
    group.sample_size(10);
    group.bench_function("single_file_reindex", |b| {
        b.iter_batched(
            || build_engine_from_project(project.path()),
            |mut engine| {
                engine.index_edited_file(
                    black_box("src/module_250.rs"),
                    black_box(&edited_content),
                    black_box(Op::Replace),
                );
                black_box((
                    engine.find_symbol("benchmark_added_symbol"),
                    engine.get_depends_on("src/module_250.rs"),
                ));
            },
            BatchSize::LargeInput,
        );
    });
    group.finish();
}

fn bench_snapshot(c: &mut Criterion) {
    let project = write_project(500);
    let engine = build_engine_from_project(project.path());
    let snapshot_dir = TempDir::new().expect("create snapshot dir");
    let snapshot_path = snapshot_dir.path().join("graph.lexa");
    snapshot::write_snapshot(&engine, &snapshot_path).expect("write initial snapshot");

    let mut group = c.benchmark_group("snapshot");
    group.sample_size(10);

    group.bench_function("write", |b| {
        b.iter(|| {
            snapshot::write_snapshot(black_box(&engine), black_box(&snapshot_path))
                .expect("write snapshot");
        });
    });

    group.bench_function("load_into_engine", |b| {
        b.iter(|| {
            let mut loaded = Engine::new(CACHE_CAPACITY);
            let count = snapshot::load_snapshot_into_engine(
                black_box(&mut loaded),
                black_box(&snapshot_path),
            )
            .expect("load snapshot");
            black_box((count, loaded.file_count(), loaded.symbol_index_count()));
        });
    });

    group.bench_function("strict_refresh_unchanged", |b| {
        b.iter_batched(
            || build_engine_from_project(project.path()),
            |mut engine| {
                black_box(
                    freshness::refresh_project(&mut engine, project.path())
                        .expect("refresh project"),
                );
            },
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_project_indexing,
    bench_search,
    bench_incremental_edit,
    bench_snapshot
);
criterion_main!(benches);
