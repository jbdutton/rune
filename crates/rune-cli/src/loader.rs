use crate::{visitor, Args, Io};
use anyhow::{anyhow, Context as _, Result};
use rune::compile::FileSourceLoader;
use rune::compile::Meta;
use rune::Diagnostics;
use rune::{Context, Hash, Options, Source, Sources, Unit};
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::{path::Path, sync::Arc};

pub(crate) struct Load {
    pub(crate) unit: Arc<Unit>,
    pub(crate) sources: Sources,
    pub(crate) functions: Vec<(Hash, Meta)>,
}

/// Load context and code for a given path
pub(crate) fn load(
    io: &mut Io<'_>,
    context: &Context,
    args: &Args,
    options: &Options,
    path: &Path,
    attribute: visitor::Attribute,
) -> Result<Load> {
    let shared = args.cmd.shared();

    let bytecode_path = path.with_extension("rnc");

    let source =
        Source::from_path(path).with_context(|| anyhow!("cannot read file: {}", path.display()))?;

    let mut sources = Sources::new();
    sources.insert(source);

    let use_cache = options.bytecode && should_cache_be_used(path, &bytecode_path)?;

    // TODO: how do we deal with tests discovery for bytecode loading
    let maybe_unit = if use_cache {
        let f = fs::File::open(&bytecode_path)?;

        match bincode::deserialize_from::<_, Unit>(f) {
            Ok(unit) => {
                log::trace!("using cache: {}", bytecode_path.display());
                Some(Arc::new(unit))
            }
            Err(e) => {
                log::error!("failed to deserialize: {}: {}", bytecode_path.display(), e);
                None
            }
        }
    } else {
        None
    };

    let (unit, functions) = match maybe_unit {
        Some(unit) => (unit, Default::default()),
        None => {
            log::trace!("building file: {}", path.display());

            let mut diagnostics = if shared.warnings {
                Diagnostics::new()
            } else {
                Diagnostics::without_warnings()
            };

            let mut functions = visitor::FunctionVisitor::new(attribute);
            let mut source_loader = FileSourceLoader::new();

            let result = rune::prepare(&mut sources)
                .with_context(context)
                .with_diagnostics(&mut diagnostics)
                .with_options(options)
                .with_visitor(&mut functions)
                .with_source_loader(&mut source_loader)
                .build();

            diagnostics.emit(io.stdout, &sources)?;
            let unit = result?;

            if options.bytecode {
                log::trace!("serializing cache: {}", bytecode_path.display());
                let f = fs::File::create(&bytecode_path)?;
                bincode::serialize_into(f, &unit)?;
            }

            (Arc::new(unit), functions.into_functions())
        }
    };

    Ok(Load {
        unit,
        sources,
        functions,
    })
}

/// Test if path `a` is newer than path `b`.
fn should_cache_be_used(source: &Path, cached: &Path) -> io::Result<bool> {
    let source = fs::metadata(source)?;

    let cached = match fs::metadata(cached) {
        Ok(cached) => cached,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };

    Ok(source.modified()? < cached.modified()?)
}

pub(crate) fn recurse_paths(
    recursive: bool,
    first: Box<Path>,
) -> impl Iterator<Item = io::Result<Box<Path>>> {
    let mut queue = VecDeque::with_capacity(1);
    queue.push_back(first);

    std::iter::from_fn(move || loop {
        let path = queue.pop_front()?;

        if !recursive {
            return Some(Ok(path));
        }

        if path.is_file() {
            if path.extension() == Some(OsStr::new("rn")) {
                return Some(Ok(path));
            }

            continue;
        }

        let d = match fs::read_dir(path) {
            Ok(d) => d,
            Err(error) => return Some(Err(error)),
        };

        for e in d {
            let e = match e {
                Ok(e) => e,
                Err(error) => return Some(Err(error)),
            };

            queue.push_back(e.path().into());
        }
    })
}
