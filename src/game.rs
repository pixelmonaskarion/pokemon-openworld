use std::{collections::HashMap, f32::consts::PI, time::{SystemTime, UNIX_EPOCH}};

use bespoke_engine::{binding::{create_layout, Descriptor, UniformBinding}, camera::Camera, instance::Instance, model::{Render, ToRaw}, shader::{Shader, ShaderConfig}, surface_context::SurfaceCtx, texture::{DepthTexture, Texture}, window::{BasicVertex, WindowConfig, WindowHandler}};
use bytemuck::{bytes_of, NoUninit};
use cgmath::{Vector2, Vector3};
use wgpu::{Limits, RenderPass, RenderPassDescriptor};
use winit::{dpi::PhysicalPosition, event::{KeyEvent, TouchPhase}, keyboard::{KeyCode, PhysicalKey::Code}};

use crate::{height_map::HeightMap, load_resource, water::Water};

pub struct Game {
    camera_binding: UniformBinding<Camera>,
    camera_pos_binding: UniformBinding<[f32; 3]>,
    camera: Camera,
    sun_camera_binding: UniformBinding<Camera>,
    screen_size: [f32; 2],
    screen_info_binding: UniformBinding<[f32; 4]>,
    time_binding: UniformBinding<f32>,
    start_time: u128,
    keys_down: Vec<KeyCode>,
    height_map: HeightMap,
    ground_shader: Shader,
    ground_shader_depth: Shader,
    touch_positions: HashMap<u64, PhysicalPosition<f64>>,
    moving_bc_finger: Option<u64>,
    water_shader: Shader,
    water: Water,
    shadow_texture: UniformBinding<DepthTexture>,
    depth_renderer_shader: Shader,
}

#[repr(C)]
#[derive(NoUninit, Copy, Clone)]
pub struct Vertex {
    pub position: [f32; 3],
    pub tex_pos: [f32; 2],
    pub normal: [f32; 3],
}

impl Vertex {
    #[allow(dead_code)]
    pub fn pos(&self) -> Vector3<f32> {
        return Vector3::new(self.position[0], self.position[1], self.position[2]);
    }
}

impl Descriptor for Vertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 5]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

impl ToRaw for Vertex {
    fn to_raw(&self) -> Vec<u8> {
        bytes_of(self).to_vec()
    }
}


impl Game {
    pub fn new(surface_context: &dyn SurfaceCtx) -> Self {
        let screen_size = [surface_context.config().width as f32, surface_context.config().height as f32];
        let screen_info_binding = UniformBinding::new(surface_context.device(), "Screen Info", [screen_size[0], screen_size[1], 0.0, 0.0], None);
        let height_image_bytes = load_resource("res/height.png").unwrap();
        // let height_map_texture = Texture::from_bytes(surface_context.device(), surface_context.queue(), &height_image_bytes, "Height Map Texture", None).unwrap();
        // let height_map = HeightMap::from_bytes_compute(device, queue, &load_resource("res/height.png").unwrap(), &height_map_texture, 2, 1.0, 250.0, true).unwrap();
        let height_map = HeightMap::from_bytes(surface_context.device(), height_image_bytes, 2, 1.0, 5, 250.0, true).unwrap();
        // let height_map = HeightMap::make_data(&height_image_bytes, 2, 1.0, 10, 250.0, true).unwrap();
        let camera = Camera {
            // eye: Vector3::new(height_map.width as f32/2.0, height_map.height_multiplier/5.0, height_map.height as f32/2.0),
            eye: Vector3::new(0.0, 0.0, 0.0),
            aspect: screen_size[0] / screen_size[1],
            fovy: 70.0,
            znear: 0.1,
            zfar: 100.0,
            ground: 0.0,
            sky: 0.0,
        };
        let camera_binding = UniformBinding::new(surface_context.device(), "Camera", camera.clone(), None);
        let camera_pos_binding = UniformBinding::new(surface_context.device(), "Camera Position", Into::<[f32; 3]>::into(camera.eye), None);
        let time_binding = UniformBinding::new(surface_context.device(), "Time", 0.0_f32, None);
        let start_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
        let ground_shader = Shader::new(include_str!("ground.wgsl"), surface_context.device(), surface_context.config().format, vec![&camera_binding.layout, &time_binding.layout], &[crate::height_map::Vertex::desc(), Instance::desc()], ShaderConfig {line_mode: wgpu::PolygonMode::Fill, ..Default::default()});
        let ground_shader_depth = Shader::new(include_str!("ground.wgsl"), surface_context.device(), surface_context.config().format, vec![&camera_binding.layout, &time_binding.layout], &[crate::height_map::Vertex::desc(), Instance::desc()], ShaderConfig {line_mode: wgpu::PolygonMode::Fill, depth_only: true, ..Default::default()});
        let water_shader = Shader::new(include_str!("water.wgsl"), surface_context.device(), surface_context.config().format, vec![&camera_binding.layout, &time_binding.layout], &[Vertex::desc(), Instance::desc()], ShaderConfig {background: false, ..Default::default()});
        let water = Water::new(surface_context.device(), height_map.width.max(height_map.height) as f32, 100.0);
        let shadow_texture = UniformBinding::new(surface_context.device(), "Shadow Depth Texture", DepthTexture::create_depth_texture(surface_context.device(), surface_context.config().width, surface_context.config().height, "Shadows Depth texture"), None);
        let depth_renderer_shader = Shader::new(include_str!("depth_renderer.wgsl"), surface_context.device(), surface_context.config().format, vec![&create_layout::<DepthTexture>(surface_context.device()), &create_layout::<DepthTexture>(surface_context.device()), &screen_info_binding.layout, &camera_binding.layout, &camera_binding.layout], &[BasicVertex::desc()], ShaderConfig {enable_depth_texture: false, ..Default::default()});
        let sun_camera_binding = UniformBinding::new(surface_context.device(), "Sun Camera", camera.clone(), None);
        Self {
            camera_binding,
            camera_pos_binding,
            camera,
            sun_camera_binding,
            screen_size,
            screen_info_binding,
            time_binding,
            start_time,
            keys_down: vec![],
            height_map,
            ground_shader,
            ground_shader_depth,
            touch_positions: HashMap::new(),
            moving_bc_finger: None,
            water,
            water_shader,
            shadow_texture,
            depth_renderer_shader,
        }
    }

    fn render_shadows(&mut self, surface_ctx: &dyn SurfaceCtx) {
        let shadow_texture = DepthTexture::create_depth_texture(surface_ctx.device(), surface_ctx.config().width, surface_ctx.config().height, "Shadows Depth texture");
        let mut encoder = surface_ctx.device().create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: None,
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &shadow_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.sun_camera_binding.set_data(surface_ctx.device(), self.sun_camera());
            if self.height_map.models.is_some() {
                self.camera_pos_binding.set_data(surface_ctx.device(), Into::<[f32; 3]>::into(self.sun_camera_binding.value.eye));
                let time = (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()-self.start_time) as f32 / 1000.0;
                self.time_binding.set_data(surface_ctx.device(), time);
                self.screen_info_binding.set_data(surface_ctx.device(), [self.screen_size[0], self.screen_size[1], time, 0.0]);
    
                render_pass.set_pipeline(&self.ground_shader_depth.pipeline);
                
                render_pass.set_bind_group(0, &self.sun_camera_binding.binding, &[]);
                render_pass.set_bind_group(1, &self.time_binding.binding, &[]);
                
                self.height_map.render(&mut render_pass);
            } else {
                self.height_map.create_models(surface_ctx.device());
            }
        }
        surface_ctx.queue().submit([encoder.finish()]);
        self.shadow_texture.set_data(surface_ctx.device(), shadow_texture);
    }

    fn sun_camera(&self) -> Camera {
        let sun_pos = Vector3::new(self.height_map.width as f32 * 1.01, 900.0, self.height_map.width as f32 * 1.01);
        let look_pos = sun_pos-Vector3::new(self.height_map.width as f32 * 0.5, 0.0, self.height_map.height as f32 * 0.5);
        let dist = (look_pos.x.powi(2)+look_pos.z.powi(2)).sqrt();
        Camera {
            eye: sun_pos,
            aspect: self.screen_size[0] / self.screen_size[1],
            fovy: 100.0,
            znear: 1.0,
            zfar: 1000.0,
            ground: (look_pos.z/look_pos.x).atan()+PI*((look_pos.x.abs()/look_pos.x)-1.0) + PI,
            sky: -(look_pos.y/dist).atan(),
        }
    }
}

impl WindowHandler for Game {
    fn resize(&mut self, _surface_ctx: &dyn SurfaceCtx, new_size: Vector2<u32>) {
        self.camera.aspect = new_size.x as f32 / new_size.y as f32;
        self.screen_size = [new_size.x as f32, new_size.y as f32];
    }

    fn render<'a: 'b, 'b>(&'a mut self, surface_ctx: &dyn SurfaceCtx, render_pass: & mut RenderPass<'b>, delta: f64) {
        let speed = 2.0 * delta as f32;
        // self.camera.ground = (self.camera.eye.z/self.camera.eye.x).atan()+PI*(self.camera.eye.x.abs()/self.camera.eye.x-1.0) + PI;
        // let dist = (self.camera.eye.x.powi(2)+self.camera.eye.z.powi(2)).sqrt();
        // self.camera.sky = -(self.camera.eye.y/dist).atan();
        if self.keys_down.contains(&KeyCode::KeyW) || self.moving_bc_finger.is_some() {
            self.camera.eye += self.camera.get_walking_vec() * speed;
        }
        if self.keys_down.contains(&KeyCode::KeyS) {
            self.camera.eye -= self.camera.get_walking_vec() * speed;
        }
        if self.keys_down.contains(&KeyCode::KeyA) {
            self.camera.eye -= self.camera.get_right_vec() * speed;
        }
        if self.keys_down.contains(&KeyCode::KeyD) {
            self.camera.eye += self.camera.get_right_vec() * speed;
        }
        if self.keys_down.contains(&KeyCode::Space) {
            self.camera.eye += Vector3::unit_y() * speed;
        }
        if self.keys_down.contains(&KeyCode::ShiftLeft) {
            self.camera.eye -= Vector3::unit_y() * speed;
        }
        // self.camera.eye.y = self.height_map.get_height_at(self.camera.eye.x, self.camera.eye.z)+2.0;
        self.render_shadows(surface_ctx);
        if self.height_map.models.is_some() {
            self.camera_binding.set_data(surface_ctx.device(), self.camera.clone());
            self.camera_pos_binding.set_data(surface_ctx.device(), Into::<[f32; 3]>::into(self.camera.eye));
            let time = (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()-self.start_time) as f32 / 1000.0;
            self.time_binding.set_data(surface_ctx.device(), time);
            self.screen_info_binding.set_data(surface_ctx.device(), [self.screen_size[0], self.screen_size[1], time, 0.0]);

            render_pass.set_pipeline(&self.ground_shader.pipeline);
            
            render_pass.set_bind_group(0, &self.camera_binding.binding, &[]);
            render_pass.set_bind_group(1, &self.time_binding.binding, &[]);
            
            self.height_map.render(render_pass);

            render_pass.set_pipeline(&self.water_shader.pipeline);
            
            self.water.model.render(render_pass);
        } else {
            self.height_map.create_models(surface_ctx.device());
        }
    }

    fn config(&self) -> Option<WindowConfig> {
        Some(WindowConfig { background_color: None, enable_post_processing: Some(true) })
    }

    fn mouse_moved(&mut self, _surface_ctx: &dyn SurfaceCtx, _mouse_pos: PhysicalPosition<f64>) {

    }
    
    fn input_event(&mut self, _surface_ctx: &dyn SurfaceCtx, input_event: &KeyEvent) {
        if let Code(code) = input_event.physical_key {
            if input_event.state.is_pressed() {
                if !self.keys_down.contains(&code) {
                    self.keys_down.push(code);
                }
            } else {
                if let Some(i) = self.keys_down.iter().position(|x| x == &code) {
                    self.keys_down.remove(i);
                }
            }
        }
    }
    
    fn mouse_motion(&mut self, _surface_ctx: &dyn SurfaceCtx, delta: (f64, f64)) {
        self.camera.ground += (delta.0 / 500.0) as f32;
        self.camera.sky -= (delta.1 / 500.0) as f32;
        self.camera.sky = self.camera.sky.clamp(std::f32::consts::PI*-0.499, std::f32::consts::PI*0.499);
    }
    
    fn touch(&mut self, surface_ctx: &dyn SurfaceCtx, touch: &winit::event::Touch) {
        match touch.phase {
            TouchPhase::Moved => {
                if let Some(last_position) = self.touch_positions.get(&touch.id) {
                    let delta = (touch.location.x-last_position.x, touch.location.y-last_position.y);
                    self.mouse_motion(surface_ctx, delta);
                    self.touch_positions.insert(touch.id, touch.location);
                }
            }
            TouchPhase::Started => {
                if touch.location.x <= self.screen_size[0] as f64 / 2.0 {
                    self.touch_positions.insert(touch.id, touch.location);
                } else {
                    self.moving_bc_finger = Some(touch.id);
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.touch_positions.remove(&touch.id);
                if self.moving_bc_finger == Some(touch.id) {
                    self.moving_bc_finger = None;
                }
            }
        }
    }
    
    fn post_process_render<'a: 'b, 'c: 'b, 'b>(&'a mut self, surface_ctx: &'c dyn SurfaceCtx, render_pass: & mut RenderPass<'b>, _surface_texture: &'c UniformBinding<Texture>) {
        render_pass.set_pipeline(&self.depth_renderer_shader.pipeline);
        render_pass.set_bind_group(0, &self.shadow_texture.binding, &[]);
        render_pass.set_bind_group(1, &surface_ctx.depth_texture().binding, &[]);
        render_pass.set_bind_group(2, &self.screen_info_binding.binding, &[]);
        render_pass.set_bind_group(3, &self.camera_binding.binding, &[]);
        render_pass.set_bind_group(4, &self.sun_camera_binding.binding, &[]);

        surface_ctx.screen_model().render(render_pass);
    }
    
    fn limits() -> wgpu::Limits {
        Limits {
            max_bind_groups: 6,
            ..Default::default()
        }
    }
    
    fn other_window_event(&mut self, _surface_ctx: &dyn SurfaceCtx, _event: &winit::event::WindowEvent) {
        
    }
    
    fn surface_config() -> Option<bespoke_engine::window::SurfaceConfig> {
        None
    }
}