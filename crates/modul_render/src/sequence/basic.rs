use bevy_ecs::prelude::*;

use crate::{Operation, OperationBuilder, RenderTargetSource};

pub struct ClearNext {
    pub render_target: RenderTargetSource,
}

impl Operation for ClearNext {
    fn run(&mut self, world: &mut World, _command_encoder: &mut wgpu::CommandEncoder) {
        self.render_target.get_mut(world).map(|mut rt| rt.schedule_clear_color());
    }
}

impl OperationBuilder for ClearNext {
    // not reading or writing, as the render target only written to when creating a pass
    fn reading(&self) -> Vec<RenderTargetSource> {
        Vec::new()
    }

    fn writing(&self) -> Vec<RenderTargetSource> {
        Vec::new()
    }

    fn finish(self, _device: &wgpu::Device) -> impl Operation + 'static {
        self
    }
}

pub struct EmptyPass {
    pub render_target: RenderTargetSource,
}

impl Operation for EmptyPass {
    fn run(&mut self, world: &mut World, command_encoder: &mut wgpu::CommandEncoder) {
        self.render_target.get_mut(world).map(|mut rt| rt.begin_ending_pass(command_encoder));
    }
}

impl OperationBuilder for EmptyPass {
    fn reading(&self) -> Vec<RenderTargetSource> {
        Vec::new()
    }

    fn writing(&self) -> Vec<RenderTargetSource> {
        vec![self.render_target]
    }

    fn finish(self, _device: &wgpu::Device) -> impl Operation + 'static {
        self
    }
}
