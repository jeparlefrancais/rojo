use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
    process::{Command, Stdio},
};

use snafu::Snafu;

use crate::cli::{InitCommand, InitKind};

static MODEL_PROJECT: &str =
    include_str!("../../assets/default-model-project/default.project.json");
static MODEL_README: &str = include_str!("../../assets/default-model-project/README.md");
static MODEL_INIT: &str = include_str!("../../assets/default-model-project/src-init.lua");
static MODEL_GIT_IGNORE: &str = include_str!("../../assets/default-model-project/gitignore.txt");

static PLACE_PROJECT: &str =
    include_str!("../../assets/default-place-project/default.project.json");
static PLACE_README: &str = include_str!("../../assets/default-place-project/README.md");
static PLACE_GIT_IGNORE: &str = include_str!("../../assets/default-place-project/gitignore.txt");

#[derive(Debug, Snafu)]
pub struct InitError(Error);

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("A project file named default.project.json already exists in this folder"))]
    AlreadyExists,

    #[snafu(display("git init failed"))]
    GitInit,

    #[snafu(display("I/O error"))]
    Io { source: io::Error },
}

impl From<io::Error> for Error {
    fn from(source: io::Error) -> Self {
        Self::Io { source }
    }
}

pub fn init(options: InitCommand) -> Result<(), InitError> {
    Ok(init_inner(options)?)
}

fn init_inner(options: InitCommand) -> Result<(), Error> {
    let base_path = options.absolute_path();
    fs::create_dir_all(&base_path)?;

    let canonical = fs::canonicalize(&base_path)?;
    let project_name = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("new-project");

    let project_params = ProjectParams {
        name: project_name.to_owned(),
    };

    match options.kind {
        InitKind::Place => init_place(&base_path, project_params),
        InitKind::Model => init_model(&base_path, project_params),
    }
}

fn init_place(base_path: &Path, project_params: ProjectParams) -> Result<(), Error> {
    eprintln!("Creating new place project '{}'", project_params.name);

    let project_file = project_params.render_template(PLACE_PROJECT);
    try_create_project(base_path, &project_file)?;

    let readme = project_params.render_template(PLACE_README);
    write_if_not_exists(&base_path.join("README.md"), &readme)?;

    let src = base_path.join("src");
    fs::create_dir_all(&src)?;

    let src_shared = src.join("shared");
    fs::create_dir_all(src.join(&src_shared))?;

    let src_server = src.join("server");
    fs::create_dir_all(src.join(&src_server))?;

    let src_client = src.join("client");
    fs::create_dir_all(src.join(&src_client))?;

    write_if_not_exists(
        &src_shared.join("Hello.lua"),
        "return function()\n\tprint(\"Hello, world!\")\nend",
    )?;

    write_if_not_exists(
        &src_server.join("init.server.lua"),
        "print(\"Hello world, from server!\")",
    )?;

    write_if_not_exists(
        &src_client.join("init.client.lua"),
        "print(\"Hello world, from client!\")",
    )?;

    let git_ignore = project_params.render_template(PLACE_GIT_IGNORE);
    try_git_init(base_path, &git_ignore)?;

    eprintln!("Created project successfully.");

    Ok(())
}

fn init_model(base_path: &Path, project_params: ProjectParams) -> Result<(), Error> {
    eprintln!("Creating new model project '{}'", project_params.name);

    let project_file = project_params.render_template(MODEL_PROJECT);
    try_create_project(base_path, &project_file)?;

    let readme = project_params.render_template(MODEL_README);
    write_if_not_exists(&base_path.join("README.md"), &readme)?;

    let src = base_path.join("src");
    fs::create_dir_all(&src)?;

    let init = project_params.render_template(MODEL_INIT);
    write_if_not_exists(&src.join("init.lua"), &init)?;

    let git_ignore = project_params.render_template(MODEL_GIT_IGNORE);
    try_git_init(base_path, &git_ignore)?;

    eprintln!("Created project successfully.");

    Ok(())
}

/// Contains parameters used in templates to create a project.
struct ProjectParams {
    name: String,
}

impl ProjectParams {
    /// Render a template by replacing variables with project parameters.
    fn render_template(&self, template: &str) -> String {
        template
            .replace("{project_name}", &self.name)
            .replace("{rojo_version}", env!("CARGO_PKG_VERSION"))
    }
}

/// Attempt to initialize a Git repository if necessary, and create .gitignore.
fn try_git_init(path: &Path, git_ignore: &str) -> Result<(), Error> {
    if should_git_init(path) {
        log::debug!("Initializing Git repository...");

        let status = Command::new("git").arg("init").current_dir(path).status()?;

        if !status.success() {
            return Err(Error::GitInit);
        }
    }

    write_if_not_exists(&path.join(".gitignore"), git_ignore)?;

    Ok(())
}

/// Tells whether we should initialize a Git repository inside the given path.
///
/// Will return false if the user doesn't have Git installed or if the path is
/// already inside a Git repository.
fn should_git_init(path: &Path) -> bool {
    let result = Command::new("git")
        .args(&["rev-parse", "--is-inside-work-tree"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(path)
        .status();

    match result {
        // If the command ran, but returned a non-zero exit code, we are not in
        // a Git repo and we should initialize one.
        Ok(status) => !status.success(),

        // If the command failed to run, we probably don't have Git installed.
        Err(_) => false,
    }
}

/// Write a file if it does not exist yet, otherwise, leave it alone.
fn write_if_not_exists(path: &Path, contents: &str) -> Result<(), Error> {
    let file_res = OpenOptions::new().write(true).create_new(true).open(path);

    let mut file = match file_res {
        Ok(file) => file,
        Err(err) => {
            return match err.kind() {
                io::ErrorKind::AlreadyExists => return Ok(()),
                _ => Err(err.into()),
            }
        }
    };

    file.write_all(contents.as_bytes())?;

    Ok(())
}

/// Try to create a project file and fail if it already exists.
fn try_create_project(base_path: &Path, contents: &str) -> Result<(), Error> {
    let project_path = base_path.join("default.project.json");

    let file_res = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(project_path);

    let mut file = match file_res {
        Ok(file) => file,
        Err(err) => {
            return match err.kind() {
                io::ErrorKind::AlreadyExists => Err(Error::AlreadyExists),
                _ => Err(err.into()),
            }
        }
    };

    file.write_all(contents.as_bytes())?;

    Ok(())
}
