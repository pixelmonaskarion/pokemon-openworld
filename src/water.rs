use bespoke_engine::{instance::Instance, model::Model};
use cgmath::{Quaternion, Rotation3, Vector3};
use wgpu::Device;

use crate::game::Vertex;

pub struct Water {
    pub model: Model,
}

impl Water {
    pub fn new(device: &Device, size: f32, height: f32) -> Self {
        let vertices = vec![
            Vertex { position: [size, height, 0.0], tex_pos: [1.0, 0.0], normal: [0.0, 0.0, 0.0] },
            Vertex { position: [size, height, size], tex_pos: [1.0, 1.0], normal: [0.0, 0.0, 0.0] },
            Vertex { position: [0.0, height, size], tex_pos: [0.0, 1.0], normal: [0.0, 0.0, 0.0] },
            Vertex { position: [0.0, height, 0.0], tex_pos: [0.0, 0.0], normal: [0.0, 0.0, 0.0] },
        ];
        let model = Model::new_instances(vertices, &[0_u16, 3, 2, 1, 0, 2], vec![
            Instance { position: Vector3::new(0.0, 0.0, 0.0), rotation: Quaternion::from_axis_angle(cgmath::Vector3::unit_z(), cgmath::Deg(0.0)) },
        ], device);
        Self {
            model
        }
    }
}