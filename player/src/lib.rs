/*! This is a player library for WebGPU traces.
 *
 * # Notes
 * - we call device_maintain_ids() before creating any refcounted resource,
 *   which is basically everything except for BGL and shader modules,
 *   so that we don't accidentally try to use the same ID.
!*/

#![warn(unsafe_op_in_unsafe_fn)]

use wgc::device::trace;

use std::{borrow::Cow, fmt::Debug, fs, marker::PhantomData, path::Path};

#[derive(Debug)]
pub struct IdentityPassThrough<I>(PhantomData<I>);

impl<I: Clone + Debug + wgc::id::TypedId> wgc::hub::IdentityHandler<I> for IdentityPassThrough<I> {
    type Input = I;
    fn process(&self, id: I, backend: wgt::Backend) -> I {
        let (index, epoch, _backend) = id.unzip();
        I::zip(index, epoch, backend)
    }
    fn free(&self, _id: I) {}
}

pub struct IdentityPassThroughFactory;

impl<I: Clone + Debug + wgc::id::TypedId> wgc::hub::IdentityHandlerFactory<I>
    for IdentityPassThroughFactory
{
    type Filter = IdentityPassThrough<I>;
    fn spawn(&self) -> Self::Filter {
        IdentityPassThrough(PhantomData)
    }
}
impl wgc::hub::GlobalIdentityHandlerFactory for IdentityPassThroughFactory {}

pub trait GlobalPlay {
    fn encode_commands<A: wgc::hub::HalApi>(
        &self,
        encoder: wgc::id::CommandEncoderId,
        commands: Vec<trace::Command>,
    ) -> wgc::id::CommandBufferId;
    fn process<A: wgc::hub::HalApi>(
        &self,
        device: wgc::id::DeviceId,
        action: trace::Action,
        dir: &Path,
        comb_manager: &mut wgc::hub::IdentityManager,
    );
}

impl GlobalPlay for wgc::hub::Global<IdentityPassThroughFactory> {
    fn encode_commands<A: wgc::hub::HalApi>(
        &self,
        encoder: wgc::id::CommandEncoderId,
        commands: Vec<trace::Command>,
    ) -> wgc::id::CommandBufferId {
        for command in commands {
            match command {
                trace::Command::CopyBufferToBuffer {
                    src,
                    src_offset,
                    dst,
                    dst_offset,
                    size,
                } => self
                    .command_encoder_copy_buffer_to_buffer::<A>(
                        encoder, src, src_offset, dst, dst_offset, size,
                    )
                    .unwrap(),
                trace::Command::CopyBufferToTexture { src, dst, size } => self
                    .command_encoder_copy_buffer_to_texture::<A>(encoder, &src, &dst, &size)
                    .unwrap(),
                trace::Command::CopyTextureToBuffer { src, dst, size } => self
                    .command_encoder_copy_texture_to_buffer::<A>(encoder, &src, &dst, &size)
                    .unwrap(),
                trace::Command::CopyTextureToTexture { src, dst, size } => self
                    .command_encoder_copy_texture_to_texture::<A>(encoder, &src, &dst, &size)
                    .unwrap(),
                trace::Command::ClearBuffer { dst, offset, size } => self
                    .command_encoder_clear_buffer::<A>(encoder, dst, offset, size)
                    .unwrap(),
                trace::Command::ClearTexture {
                    dst,
                    subresource_range,
                } => self
                    .command_encoder_clear_texture::<A>(encoder, dst, &subresource_range)
                    .unwrap(),
                trace::Command::WriteTimestamp {
                    query_set_id,
                    query_index,
                } => self
                    .command_encoder_write_timestamp::<A>(encoder, query_set_id, query_index)
                    .unwrap(),
                trace::Command::ResolveQuerySet {
                    query_set_id,
                    start_query,
                    query_count,
                    destination,
                    destination_offset,
                } => self
                    .command_encoder_resolve_query_set::<A>(
                        encoder,
                        query_set_id,
                        start_query,
                        query_count,
                        destination,
                        destination_offset,
                    )
                    .unwrap(),
                trace::Command::PushDebugGroup(marker) => self
                    .command_encoder_push_debug_group::<A>(encoder, &marker)
                    .unwrap(),
                trace::Command::PopDebugGroup => {
                    self.command_encoder_pop_debug_group::<A>(encoder).unwrap()
                }
                trace::Command::InsertDebugMarker(marker) => self
                    .command_encoder_insert_debug_marker::<A>(encoder, &marker)
                    .unwrap(),
                trace::Command::RunComputePass { base } => {
                    self.command_encoder_run_compute_pass_impl::<A>(encoder, base.as_ref())
                        .unwrap();
                }
                trace::Command::RunRenderPass {
                    base,
                    target_colors,
                    target_depth_stencil,
                } => {
                    self.command_encoder_run_render_pass_impl::<A>(
                        encoder,
                        base.as_ref(),
                        &target_colors,
                        target_depth_stencil.as_ref(),
                    )
                    .unwrap();
                }
                trace::Command::BuildAccelerationStructuresUnsafeTlas { blas, tlas } => {
                    let blas_iter = (&blas).into_iter().map(|x| {
                        let geometries = match &x.geometries {
                            wgc::ray_tracing::TraceBlasGeometries::TriangleGeometries(
                                triangle_geometries,
                            ) => {
                                let iter = triangle_geometries.into_iter().map(|tg| {
                                    wgc::ray_tracing::BlasTriangleGeometry {
                                        size: &tg.size,
                                        vertex_buffer: tg.vertex_buffer,
                                        index_buffer: tg.index_buffer,
                                        transform_buffer: tg.transform_buffer,
                                        first_vertex: tg.first_vertex,
                                        vertex_stride: tg.vertex_stride,
                                        index_buffer_offset: tg.index_buffer_offset,
                                        transform_buffer_offset: tg.transform_buffer_offset,
                                    }
                                });
                                wgc::ray_tracing::BlasGeometries::TriangleGeometries(Box::new(iter))
                            }
                        };
                        wgc::ray_tracing::BlasBuildEntry {
                            blas_id: x.blas_id,
                            geometries: geometries,
                        }
                    });

                    if !tlas.is_empty() {
                        log::error!("a trace of command_encoder_build_acceleration_structures_unsafe_tlas containing a tlas build is not replayable! skipping tlas build");
                    }

                    self.command_encoder_build_acceleration_structures_unsafe_tlas::<A>(
                        encoder,
                        blas_iter,
                        std::iter::empty(),
                    )
                    .unwrap();
                }
            }
        }
        let (cmd_buf, error) = self
            .command_encoder_finish::<A>(encoder, &wgt::CommandBufferDescriptor { label: None });
        if let Some(e) = error {
            panic!("{:?}", e);
        }
        cmd_buf
    }

    fn process<A: wgc::hub::HalApi>(
        &self,
        device: wgc::id::DeviceId,
        action: trace::Action,
        dir: &Path,
        comb_manager: &mut wgc::hub::IdentityManager,
    ) {
        use wgc::device::trace::Action;
        log::info!("action {:?}", action);
        //TODO: find a way to force ID perishing without excessive `maintain()` calls.
        match action {
            Action::Init { .. } => {
                panic!("Unexpected Action::Init: has to be the first action only")
            }
            Action::ConfigureSurface { .. }
            | Action::Present(_)
            | Action::DiscardSurfaceTexture(_) => {
                panic!("Unexpected Surface action: winit feature is not enabled")
            }
            Action::CreateBuffer(id, desc) => {
                self.device_maintain_ids::<A>(device).unwrap();
                let (_, error) = self.device_create_buffer::<A>(device, &desc, id);
                if let Some(e) = error {
                    panic!("{:?}", e);
                }
            }
            Action::FreeBuffer(id) => {
                self.buffer_destroy::<A>(id).unwrap();
            }
            Action::DestroyBuffer(id) => {
                self.buffer_drop::<A>(id, true);
            }
            Action::CreateTexture(id, desc) => {
                self.device_maintain_ids::<A>(device).unwrap();
                let (_, error) = self.device_create_texture::<A>(device, &desc, id);
                if let Some(e) = error {
                    panic!("{:?}", e);
                }
            }
            Action::FreeTexture(id) => {
                self.texture_destroy::<A>(id).unwrap();
            }
            Action::DestroyTexture(id) => {
                self.texture_drop::<A>(id, true);
            }
            Action::CreateTextureView {
                id,
                parent_id,
                desc,
            } => {
                self.device_maintain_ids::<A>(device).unwrap();
                let (_, error) = self.texture_create_view::<A>(parent_id, &desc, id);
                if let Some(e) = error {
                    panic!("{:?}", e);
                }
            }
            Action::DestroyTextureView(id) => {
                self.texture_view_drop::<A>(id, true).unwrap();
            }
            Action::CreateSampler(id, desc) => {
                self.device_maintain_ids::<A>(device).unwrap();
                let (_, error) = self.device_create_sampler::<A>(device, &desc, id);
                if let Some(e) = error {
                    panic!("{:?}", e);
                }
            }
            Action::DestroySampler(id) => {
                self.sampler_drop::<A>(id);
            }
            Action::GetSurfaceTexture { id, parent_id } => {
                self.device_maintain_ids::<A>(device).unwrap();
                self.surface_get_current_texture::<A>(parent_id, id)
                    .unwrap()
                    .texture_id
                    .unwrap();
            }
            Action::CreateBindGroupLayout(id, desc) => {
                let (_, error) = self.device_create_bind_group_layout::<A>(device, &desc, id);
                if let Some(e) = error {
                    panic!("{:?}", e);
                }
            }
            Action::DestroyBindGroupLayout(id) => {
                self.bind_group_layout_drop::<A>(id);
            }
            Action::CreatePipelineLayout(id, desc) => {
                self.device_maintain_ids::<A>(device).unwrap();
                let (_, error) = self.device_create_pipeline_layout::<A>(device, &desc, id);
                if let Some(e) = error {
                    panic!("{:?}", e);
                }
            }
            Action::DestroyPipelineLayout(id) => {
                self.pipeline_layout_drop::<A>(id);
            }
            Action::CreateBindGroup(id, desc) => {
                self.device_maintain_ids::<A>(device).unwrap();
                let (_, error) = self.device_create_bind_group::<A>(device, &desc, id);
                if let Some(e) = error {
                    panic!("{:?}", e);
                }
            }
            Action::DestroyBindGroup(id) => {
                self.bind_group_drop::<A>(id);
            }
            Action::CreateShaderModule { id, desc, data } => {
                log::info!("Creating shader from {}", data);
                let code = fs::read_to_string(dir.join(&data)).unwrap();
                let source = if data.ends_with(".wgsl") {
                    wgc::pipeline::ShaderModuleSource::Wgsl(Cow::Owned(code))
                } else if data.ends_with(".ron") {
                    let module = ron::de::from_str(&code).unwrap();
                    wgc::pipeline::ShaderModuleSource::Naga(module)
                } else {
                    panic!("Unknown shader {}", data);
                };
                let (_, error) = self.device_create_shader_module::<A>(device, &desc, source, id);
                if let Some(e) = error {
                    panic!("{:?}", e);
                }
            }
            Action::DestroyShaderModule(id) => {
                self.shader_module_drop::<A>(id);
            }
            Action::CreateComputePipeline {
                id,
                desc,
                implicit_context,
            } => {
                self.device_maintain_ids::<A>(device).unwrap();
                let implicit_ids =
                    implicit_context
                        .as_ref()
                        .map(|ic| wgc::device::ImplicitPipelineIds {
                            root_id: ic.root_id,
                            group_ids: &ic.group_ids,
                        });
                let (_, error) =
                    self.device_create_compute_pipeline::<A>(device, &desc, id, implicit_ids);
                if let Some(e) = error {
                    panic!("{:?}", e);
                }
            }
            Action::DestroyComputePipeline(id) => {
                self.compute_pipeline_drop::<A>(id);
            }
            Action::CreateRenderPipeline {
                id,
                desc,
                implicit_context,
            } => {
                self.device_maintain_ids::<A>(device).unwrap();
                let implicit_ids =
                    implicit_context
                        .as_ref()
                        .map(|ic| wgc::device::ImplicitPipelineIds {
                            root_id: ic.root_id,
                            group_ids: &ic.group_ids,
                        });
                let (_, error) =
                    self.device_create_render_pipeline::<A>(device, &desc, id, implicit_ids);
                if let Some(e) = error {
                    panic!("{:?}", e);
                }
            }
            Action::DestroyRenderPipeline(id) => {
                self.render_pipeline_drop::<A>(id);
            }
            Action::CreateRenderBundle { id, desc, base } => {
                let bundle =
                    wgc::command::RenderBundleEncoder::new(&desc, device, Some(base)).unwrap();
                let (_, error) = self.render_bundle_encoder_finish::<A>(
                    bundle,
                    &wgt::RenderBundleDescriptor { label: desc.label },
                    id,
                );
                if let Some(e) = error {
                    panic!("{:?}", e);
                }
            }
            Action::DestroyRenderBundle(id) => {
                self.render_bundle_drop::<A>(id);
            }
            Action::CreateQuerySet { id, desc } => {
                self.device_maintain_ids::<A>(device).unwrap();
                let (_, error) = self.device_create_query_set::<A>(device, &desc, id);
                if let Some(e) = error {
                    panic!("{:?}", e);
                }
            }
            Action::DestroyQuerySet(id) => {
                self.query_set_drop::<A>(id);
            }
            Action::WriteBuffer {
                id,
                data,
                range,
                queued,
            } => {
                let bin = std::fs::read(dir.join(data)).unwrap();
                let size = (range.end - range.start) as usize;
                if queued {
                    self.queue_write_buffer::<A>(device, id, range.start, &bin)
                        .unwrap();
                } else {
                    self.device_wait_for_buffer::<A>(device, id).unwrap();
                    self.device_set_buffer_sub_data::<A>(device, id, range.start, &bin[..size])
                        .unwrap();
                }
            }
            Action::WriteTexture {
                to,
                data,
                layout,
                size,
            } => {
                let bin = std::fs::read(dir.join(data)).unwrap();
                self.queue_write_texture::<A>(device, &to, &bin, &layout, &size)
                    .unwrap();
            }
            Action::Submit(_index, ref commands) if commands.is_empty() => {
                self.queue_submit::<A>(device, &[]).unwrap();
            }
            Action::Submit(_index, commands) => {
                let (encoder, error) = self.device_create_command_encoder::<A>(
                    device,
                    &wgt::CommandEncoderDescriptor { label: None },
                    comb_manager.alloc(device.backend()),
                );
                if let Some(e) = error {
                    panic!("{:?}", e);
                }
                let cmdbuf = self.encode_commands::<A>(encoder, commands);
                self.queue_submit::<A>(device, &[cmdbuf]).unwrap();
            }
            Action::CreateBlas { id, desc, sizes } => {
                self.device_maintain_ids::<A>(device).unwrap();
                self.device_create_blas::<A>(device, &desc, sizes, id);
            }
            Action::CreateTlas { id, desc } => {
                self.device_maintain_ids::<A>(device).unwrap();
                self.device_create_tlas::<A>(device, &desc, id);
            }
        }
    }
}
