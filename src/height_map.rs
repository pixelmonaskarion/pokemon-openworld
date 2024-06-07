use std::sync::mpsc::{channel, Receiver};

use bespoke_engine::{binding::Descriptor, compute::ComputeShader, instance::Instance, model::{Model, Render, ToRaw}, texture::Texture};
use bytemuck::{bytes_of, NoUninit};
use cgmath::{Deg, InnerSpace, Quaternion, Rotation3, Vector3};
use image::{DynamicImage, GenericImageView, ImageError};
use wgpu::{util::DeviceExt, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor, Device, Queue};

#[repr(C)]
#[derive(NoUninit, Copy, Clone)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub normal: [f32; 3],
}

impl Vertex {
    pub fn pos(&self) -> Vector3<f32> {
        return Vector3::new(self.position[0], self.position[1], self.position[2]);
    }
}

impl Descriptor for Vertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
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
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
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

pub struct HeightMap {
    pub image: Option<DynamicImage>,
    pub models: Option<Vec<((u32, u32), Model)>>,
    pub model_data_recv: Option<Receiver<(Vec<((u32, u32), (Vec<Vertex>, Vec<u32>))>, DynamicImage)>>,
    pub width: u32,
    pub height: u32,
    pub size: f32,
    pub height_multiplier: f32,
}

impl HeightMap {
    pub fn from_bytes(device: &Device, image_bytes: &[u8], res: u32, size: f32, chunks: u32, height_multiplier: f32, gen_normals: bool) -> Result<Self, ImageError> {
        let image = image::load_from_memory(image_bytes)?.grayscale();
        let width = image.width()/res;
        let height = image.height()/res;
        let mut models = Vec::new();
        for cx in 0..chunks {
            for cy in 0..chunks {
                let mut vertices = vec![];
                let mut indices = vec![];
                let extra_x = if cx == chunks-1 {
                    0
                } else {
                    1
                };
                let extra_y = if cy == chunks-1 {
                    0
                } else {
                    1
                };
                for x in 0..width/chunks+extra_x {
                    for y in 0..height/chunks+extra_y {
                        let px = x + (width/chunks)*cx;
                        let py = y + (height/chunks)*cy;
                        let v_height = image.get_pixel(px*res, py*res).0[0] as f32 / 255.0 * height_multiplier;
                        let mut color = [17.0/255.0,124.0/255.0,19.0/255.0];
                        if v_height > height_multiplier*0.7 {
                            color = [0.9, 0.9, 0.9];
                        }
                        if v_height <= 0.1439215686*height_multiplier {
                            color = [0.3, 0.3, 0.3];
                        }
                        vertices.push(Vertex { position: [(px*res) as f32 * size, v_height, (py*res) as f32 * size], color, normal: [0.0, 1.0, 0.0] });
                        if x < (width/chunks+extra_x)-1 && y < (height/chunks+extra_y)-1 {
                            let i = x * (height/chunks+extra_y) + y;
                            indices.append(&mut [i, i+1, i+(height/chunks+extra_y)+1, i, i+(height/chunks+extra_y)+1, i+(height/chunks+extra_y)].to_vec());
                        }
                    }
                }
                if gen_normals {
                    for i in 0..indices.len()/3 {
                        let v1 = indices[i*3] as usize;
                        let v2 = indices[i*3+1] as usize;
                        let v3 = indices[i*3+2] as usize;
        
                        let u = vertices[v2].pos()-vertices[v1].pos();
                        let v = vertices[v3].pos()-vertices[v1].pos();
        
                        let mut normal = Vector3::new(0.0, 0.0, 0.0);
                        normal.x = u.y*v.z - u.z*v.y;
                        normal.y = u.z*v.x - u.x*v.z;
                        normal.z = u.x*v.y - u.y*v.x;
                        normal = normal.normalize();
                        vertices[v1].normal = normal.into();
                        vertices[v2].normal = normal.into();
                        vertices[v3].normal = normal.into();
                        if normal.y < 0.5 {
                            let dirt_color = [165.0/255.0,42.0/255.0,42.0/255.0];
                            if vertices[v1].color != [0.9, 0.9, 0.9] { vertices[v1].color = dirt_color; } 
                            if vertices[v2].color != [0.9, 0.9, 0.9] { vertices[v2].color = dirt_color; } 
                            if vertices[v3].color != [0.9, 0.9, 0.9] { vertices[v3].color = dirt_color; } 
                        }
                    }
                }
                let model = Model::new_instances(vertices, &indices, vec![
                    // Instance {rotation: Quaternion::zero(), position: vec3(x, y, z)},
                    Instance::default(),
                ], device);
                models.push(((cx, cy), model));
            }
        }
        
        Ok(Self {
            models: Some(models),
            model_data_recv: None,
            width: image.width(),
            height: image.height(),
            size,
            image: Some(image),
            height_multiplier,
        })
    }

    pub fn make_data(image_bytes: &[u8], res: u32, size: f32, chunks: u32, height_multiplier: f32, gen_normals: bool) -> Result<Self, ImageError> {
        let image = image::load_from_memory(image_bytes)?.grayscale();
        let image_width = image.width();
        let image_height = image.height();
        let width = image.width()/res;
        let height = image.height()/res;
        let (sender, recv) = channel();
        std::thread::spawn(move || {
            let mut model_data = Vec::new();
            for cx in 0..chunks {
                for cy in 0..chunks {
                    let mut vertices = vec![];
                    let mut indices = vec![];
                    let extra_x = if cx == chunks-1 {
                        0
                    } else {
                        1
                    };
                    let extra_y = if cy == chunks-1 {
                        0
                    } else {
                        1
                    };
                    for x in 0..width/chunks+extra_x {
                        for y in 0..height/chunks+extra_y {
                            let px = x + (width/chunks)*cx;
                            let py = y + (height/chunks)*cy;
                            let v_height = image.get_pixel(px*res, py*res).0[0] as f32 / 255.0 * height_multiplier;
                            let mut color = [17.0/255.0,124.0/255.0,19.0/255.0];
                            if v_height > height_multiplier*0.7 {
                                color = [0.9, 0.9, 0.9];
                            }
                            if v_height <= 0.1439215686*height_multiplier {
                                color = [0.3, 0.3, 0.3];
                            }
                            vertices.push(Vertex { position: [(px*res) as f32 * size, v_height, (py*res) as f32 * size], color, normal: [0.0, 1.0, 0.0] });
                            if x < (width/chunks+extra_x)-1 && y < (height/chunks+extra_y)-1 {
                                let i = x * (height/chunks+extra_y) + y;
                                indices.append(&mut [i, i+1, i+(height/chunks+extra_y)+1, i, i+(height/chunks+extra_y)+1, i+(height/chunks+extra_y)].to_vec());
                            }
                        }
                    }
                    if gen_normals {
                        for i in 0..indices.len()/3 {
                            let v1 = indices[i*3] as usize;
                            let v2 = indices[i*3+1] as usize;
                            let v3 = indices[i*3+2] as usize;
                            
                            let u = vertices[v2].pos()-vertices[v1].pos();
                            let v = vertices[v3].pos()-vertices[v1].pos();
                            
                            let mut normal = Vector3::new(0.0, 0.0, 0.0);
                            normal.x = u.y*v.z - u.z*v.y;
                            normal.y = u.z*v.x - u.x*v.z;
                            normal.z = u.x*v.y - u.y*v.x;
                            normal = normal.normalize();
                            vertices[v1].normal = normal.into();
                            vertices[v2].normal = normal.into();
                            vertices[v3].normal = normal.into();
                            if normal.y < 0.5 {
                                let dirt_color = [165.0/255.0,42.0/255.0,42.0/255.0];
                                if vertices[v1].color != [0.9, 0.9, 0.9] { vertices[v1].color = dirt_color; } 
                                if vertices[v2].color != [0.9, 0.9, 0.9] { vertices[v2].color = dirt_color; } 
                                if vertices[v3].color != [0.9, 0.9, 0.9] { vertices[v3].color = dirt_color; } 
                            }
                        }
                    }
                    model_data.push(((cx, cy), (vertices, indices)));
                }
            }
            sender.send((model_data, image)).unwrap();
        });
        Ok(Self {
            models: None,
            model_data_recv: Some(recv),
            width: image_width,
            height: image_height,
            size,
            image: None,
            height_multiplier,
        })
    }

    pub fn from_bytes_compute(device: &Device, queue: &Queue, image_bytes: &[u8], image_texture: &Texture, res: u32, size: f32, height_multiplier: f32, gen_normals: bool) -> Result<Self, ImageError> {
        let image = image::load_from_memory(image_bytes)?.grayscale();
        let width = image_texture.texture.width()/res;
        let height = image_texture.texture.height()/res;
        println!("HEIGHT: {height}");
        let mut indices = vec![];
        let mut vertices = vec![];
        for x in 0..width {
            for y in 0..height {
                vertices.push(Vertex {color: [1.0, 1.0, 1.0], normal: [0.0, 1.0, 0.0], position: [(x*res) as f32 * size, 1.0, (y*res) as f32 * size]});
                if x < width-1 && y < height-1 {
                    let i = x * height + y;
                    indices.append(&mut [i, i+1, i+height+1, i, i+height+1, i+height].to_vec());
                }
            }
        }
        // let blank_vertices: Vec<_> = vec![Vertex {color: [1.0, 1.0, 1.0], normal: [0.0, 1.0, 0.0], position: [0.0, 0.0, 0.0]}; (width*height) as usize];
        let src_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Height Map Source Vertex Buffer")),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX,
        });
        let dst_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Height Map Output Vertex Buffer")),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX,
        });
        let dst_layout = 
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage {
                            read_only: true,
                        },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }, wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage {
                            read_only: false,
                        },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }]
            });
        let dst_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &dst_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: src_buffer.as_entire_binding(),
            }, BindGroupEntry {
                binding: 1,
                resource: dst_buffer.as_entire_binding(),
            }]
        });
        let compute_shader = ComputeShader::new(include_str!("height_gen.wgsl"), &[&dst_layout], device);
        compute_shader.run(&[&dst_bind_group], [width, height, 1], device, queue);
        let model = Model::new_vertex_buffer(dst_buffer, width*height, vec![Instance {position: Vector3::new(0.0, 0.0, 0.0), rotation: Quaternion::from_axis_angle(Vector3::unit_z(), Deg(0.0))}], &indices, device);
        Ok(Self {
            models: Some(vec![((0, 0), model)]),
            model_data_recv: None,
            width: image_texture.texture.width(),
            height: image_texture.texture.height(),
            size,
            image: Some(image),
            height_multiplier,
        })
    }

    pub fn get_height_at(&self, x: f32, y: f32) -> f32 {
        if let Some(image) = &self.image {
            let x = (x/self.size).clamp(0.0, self.width as f32 - 2.0);
            let y = (y/self.size).clamp(0.0, self.height as f32 - 2.0);
            let x_fract = x.fract();
            let y_fract = y.fract();
            let x = x.floor() as u32;
            let y = y.floor() as u32;
            let height0 = image.get_pixel(x, y).0[0] as f32 / 255.0 * self.height_multiplier;
            let height1 = image.get_pixel(x, y+1).0[0] as f32 / 255.0 * self.height_multiplier;
            let height2 = image.get_pixel(x+1, y).0[0] as f32 / 255.0 * self.height_multiplier;
            let height3 = image.get_pixel(x+1, y+1).0[0] as f32 / 255.0 * self.height_multiplier;
            let heightx1 = height0+(height1-height0)*x_fract;
            let heightx2 = height2+(height3-height2)*x_fract;
            return heightx1 + (heightx2-heightx1)*y_fract;
        } else {
            return 0.0;
        }
    }

    pub fn create_models(&mut self, device: &Device) {
        let model_data = self.model_data_recv.as_ref().map(|recv| {
            recv.recv().ok()
        }).flatten();
        if let Some(model_data) = model_data {
            self.image = Some(model_data.1);
            self.models = Some(model_data.0.into_iter().map(|model_data| {
                (model_data.0, Model::new_instances(model_data.1.0, &model_data.1.1, vec![Instance::default()], device))
            }).collect());
        }
    }
}

impl Render for HeightMap {
    fn render<'a: 'b, 'b>(&'a self, render_pass: &mut wgpu::RenderPass<'b>) {
        if let Some(models) = &self.models {
            for (_, model) in models {
                model.render(render_pass);
            }
        }
    }
    fn render_instances<'a: 'b, 'c: 'b, 'b>(&'a self, render_pass: &mut wgpu::RenderPass<'b>, instances: &'c wgpu::Buffer, range: std::ops::Range<u32>) {
        if let Some(models) = &self.models {
            for (_, model) in models {
                model.render_instances(render_pass, instances, range.clone());
            }
        }
    }
}