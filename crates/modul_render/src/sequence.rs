use crate::render_target::{OffscreenRenderTarget, RenderTarget, SurfaceRenderTarget};
use bevy_ecs::prelude::*;
use modul_asset::{AssetId, Assets};
use modul_core::{DeviceRes, QueueRes};
use std::iter;
use std::ops::{Deref, DerefMut};
use wgpu::{CommandEncoder, CommandEncoderDescriptor, Device};
mod basic;

pub use basic::*;
use modul_util::HashSet;

#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub enum RenderTargetSource {
    Surface(Entity),
    Offscreen(Entity),
}

impl RenderTargetSource {
    pub fn get<'a>(&'a self, world: &'a World) -> Option<&'a dyn RenderTarget> {
        match self {
            RenderTargetSource::Surface(e) => world
                .get::<SurfaceRenderTarget>(*e)
                .map(|t| t as &dyn RenderTarget),
            RenderTargetSource::Offscreen(e) => world
                .get::<OffscreenRenderTarget>(*e)
                .map(|t| t as &dyn RenderTarget),
        }
    }

    pub fn get_mut<'a>(&'a self, world: &'a mut World) -> Option<RenderTargetMut<'a>> {
        match self {
            RenderTargetSource::Surface(e) => {
                world.get_mut(*e).map(|rt| RenderTargetMut::Surface(rt))
            }
            RenderTargetSource::Offscreen(e) => {
                world.get_mut(*e).map(|rt| RenderTargetMut::Offscreen(rt))
            }
        }
    }
}

pub enum RenderTargetMut<'a> {
    Surface(Mut<'a, SurfaceRenderTarget>),
    Offscreen(Mut<'a, OffscreenRenderTarget>),
}

impl<'a> Deref for RenderTargetMut<'a> {
    type Target = dyn RenderTarget;

    fn deref(&self) -> &Self::Target {
        match self {
            RenderTargetMut::Surface(s) => s.as_ref(),
            RenderTargetMut::Offscreen(s) => s.as_ref(),
        }
    }
}

impl<'a> DerefMut for RenderTargetMut<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            RenderTargetMut::Surface(s) => s.as_mut(),
            RenderTargetMut::Offscreen(s) => s.as_mut(),
        }
    }
}

pub trait OperationBuilder: Send + Sync + 'static {
    /// Used by the sequence to determine when to resolve rendertargets
    fn reading(&self) -> Vec<RenderTargetSource>;
    /// used by the sequence to determine when to resolve rendertargets
    fn writing(&self) -> Vec<RenderTargetSource>;
    fn finish(self, device: &Device) -> impl Operation + 'static;
}

pub trait Operation: Send + Sync {
    fn run(&mut self, world: &mut World, command_encoder: &mut CommandEncoder);
}

pub struct Sequence {
    // to not have Sequence publicly be an enum
    inner: InnerSequence,
}

impl Sequence {
    fn run(&mut self, command_encoder: &mut CommandEncoder, world: &mut World) {
        if let InnerSequence::UnInitialized(builders) = &mut self.inner {
            let device = &world.resource::<DeviceRes>().0;
            let mut operations = Vec::new();
            let mut needs_resolving = HashSet::<RenderTargetSource>::new();
            for builder in builders {
                for reading in builder.reading() {
                    if needs_resolving.contains(&reading) {
                        needs_resolving.remove(&reading);
                        operations.push(SequenceOperation::ResolveNext(reading));
                    }
                }
                for writing in builder.writing() {
                    needs_resolving.insert(writing);
                }
                operations.push(SequenceOperation::Run(builder.finish(device)));
            }
            for resolve in needs_resolving {
                operations.push(SequenceOperation::ResolveNext(resolve));
            }
            self.inner = InnerSequence::Ready(operations);
        }
        // should always be true, not using match as this will run after the other if let
        if let InnerSequence::Ready(ops) = &mut self.inner {
            for op in ops.iter_mut() {
                match op {
                    SequenceOperation::ResolveNext(target) => {
                        target.get_mut(world).map(|mut rt| rt.schedule_resolve());
                    }
                    SequenceOperation::Run(op) => {
                        op.run(world, command_encoder);
                    }
                }
            }
        }
    }
}

pub struct SequenceBuilder {
    operation_builders: Vec<Box<dyn DynOperationBuilder>>,
}

impl SequenceBuilder {
    pub fn new() -> SequenceBuilder {
        SequenceBuilder {
            operation_builders: vec![],
        }
    }

    pub fn add(&mut self, operation_builder: impl OperationBuilder) -> &mut Self {
        self.operation_builders
            .push(Box::new(DynOperationBuilderImpl(Some(Box::new(
                operation_builder,
            )))));
        self
    }

    pub fn finish(self, assets: &mut Assets<Sequence>) -> AssetId<Sequence> {
        assets.add(Sequence {
            inner: InnerSequence::UnInitialized(self.operation_builders),
        })
    }
}

pub enum SequenceOperation {
    Run(Box<dyn Operation>),
    ResolveNext(RenderTargetSource),
}

pub struct SequenceQueue(pub Vec<AssetId<Sequence>>);

impl From<Vec<SequenceQueue>> for SequenceQueue {
    fn from(value: Vec<SequenceQueue>) -> Self {
        Self(value.into_iter().map(|seq| seq.0).flatten().collect())
    }
}

#[derive(Resource)]
pub struct RunningSequenceQueue(pub SequenceQueue);

// to get around dyn not being able to consume self
// maybe there is a better way to do this
trait DynOperationBuilder: Send + Sync + 'static {
    fn reading(&self) -> Vec<RenderTargetSource>;
    fn writing(&self) -> Vec<RenderTargetSource>;
    fn finish(&mut self, device: &Device) -> Box<dyn Operation>;
}

struct DynOperationBuilderImpl<T: OperationBuilder>(Option<Box<T>>);

impl<T: OperationBuilder> DynOperationBuilder for DynOperationBuilderImpl<T> {
    fn reading(&self) -> Vec<RenderTargetSource> {
        self.0.as_ref().unwrap().reading()
    }

    fn writing(&self) -> Vec<RenderTargetSource> {
        self.0.as_ref().unwrap().writing()
    }

    fn finish(&mut self, device: &Device) -> Box<dyn Operation> {
        Box::new(self.0.take().unwrap().finish(device))
    }
}
enum InnerSequence {
    Ready(Vec<SequenceOperation>),
    UnInitialized(Vec<Box<dyn DynOperationBuilder>>),
}

pub(crate) fn run_sequences(world: &mut World) {
    world.resource_scope(|world, mut sequence_assets: Mut<Assets<Sequence>>| {
        if !world.contains_resource::<RunningSequenceQueue>() {
            return;
        }
        world.resource_scope(|world, sequence_queue: Mut<RunningSequenceQueue>| {
            // FIXME maybe use multiple command encoders and run in parallel??
            let mut command_encoder =
                world
                    .resource::<DeviceRes>()
                    .0
                    .create_command_encoder(&CommandEncoderDescriptor {
                        label: Some("Sequence runner encoder"),
                    });
            for asset_id in &sequence_queue.0 .0 {
                sequence_assets
                    .get_mut(*asset_id)
                    .expect("sequence was added to queue, but does not exist")
                    .run(&mut command_encoder, world)
            }
            world
                .resource::<QueueRes>()
                .0
                .submit(iter::once(command_encoder.finish()));
        });
    });
}
