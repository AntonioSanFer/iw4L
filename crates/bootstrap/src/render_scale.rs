//! Draws the 3D view at a fraction of the window's pixels and stretches it
//! over the window. On a phone the GPU's cost follows the pixel count, and the
//! panel is dense enough that 2/3 per side is hard to see. The HUD and menus
//! keep drawing on their own window cameras at full resolution.
//!
//! The lens renders into an image; a camera below the HUD's draws that image
//! as one full-window UI node. At scale 1 the lens draws to the window and
//! none of this exists. `IW4L_RENDER_SCALE` overrides `render_scale`.

use std::sync::OnceLock;

use bevy::camera::RenderTarget;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::Msaa;
use bevy::window::{PrimaryWindow, WindowRef};
use render_scene::FpvLens;

const RENDER_SCALE_ENV: &str = "IW4L_RENDER_SCALE";

/// Above the lens (0), below the loading screen (100) and the HUD (200).
const PRESENT_ORDER: isize = 1;

/// What the lens draws at, for the frame log.
#[derive(Resource, Default)]
pub(crate) struct SceneScale {
    current: Option<(Handle<Image>, UVec2)>,
}

impl SceneScale {
    pub(crate) fn describe(&self) -> String {
        match &self.current {
            Some((_, size)) => format!("{}x{}", size.x, size.y),
            None => "window".to_owned(),
        }
    }
}

#[derive(Component)]
struct ScenePresentCamera;

#[derive(Component)]
struct ScenePresentNode;

fn render_scale(settings: &frame::GameSettings) -> f32 {
    static SCALE: OnceLock<Option<f32>> = OnceLock::new();
    SCALE
        .get_or_init(|| crate::frame_pacing::env_number(RENDER_SCALE_ENV))
        .unwrap_or(settings.render_scale)
        .clamp(frame::GameSettings::RENDER_SCALE_MIN, 1.0)
}

/// The scaled size, kept even so a 2:1 upscale stays on whole pixels.
fn scaled_size(window: &Window, scale: f32) -> UVec2 {
    let side = |physical: u32| ((physical as f32 * scale / 2.0).round() as u32 * 2).max(2);
    UVec2::new(
        side(window.physical_width()),
        side(window.physical_height()),
    )
}

fn scene_image(size: UVec2) -> Image {
    let mut image = Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
    image.sampler = ImageSampler::linear();
    image
}

#[allow(clippy::too_many_arguments)]
fn apply_render_scale(
    mut commands: Commands,
    settings: Res<frame::GameSettings>,
    windows: Query<&Window, With<PrimaryWindow>>,
    lenses: Query<Entity, With<FpvLens>>,
    cameras: Query<Entity, With<ScenePresentCamera>>,
    mut nodes: Query<(Entity, &mut ImageNode), With<ScenePresentNode>>,
    mut images: ResMut<Assets<Image>>,
    mut scale: ResMut<SceneScale>,
) {
    let wanted = render_scale(&settings);
    let lens = lenses.single().ok();
    let size = match (lens, windows.single()) {
        (Some(_), Ok(window)) if wanted < 1.0 => Some(scaled_size(window, wanted)),
        _ => None,
    };
    let Some(size) = size else {
        if scale.current.take().is_some() {
            if let Some(lens) = lens {
                commands
                    .entity(lens)
                    .insert(RenderTarget::Window(WindowRef::Primary));
            }
            for entity in cameras.iter().chain(nodes.iter().map(|(entity, _)| entity)) {
                commands.entity(entity).despawn();
            }
        }
        return;
    };
    let Some(lens) = lens else {
        return;
    };
    if scale
        .current
        .as_ref()
        .is_some_and(|(_, current)| *current == size)
    {
        return;
    }
    let handle = images.add(scene_image(size));
    commands
        .entity(lens)
        .insert(RenderTarget::Image(handle.clone().into()));
    let camera = cameras.single().ok().unwrap_or_else(|| {
        commands
            .spawn((
                Camera2d,
                Camera {
                    order: PRESENT_ORDER,
                    clear_color: ClearColorConfig::Custom(Color::BLACK),
                    ..default()
                },
                Msaa::Off,
                ScenePresentCamera,
            ))
            .id()
    });
    match nodes.single_mut() {
        Ok((_, mut node)) => node.image = handle.clone(),
        Err(_) => {
            commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                ImageNode::new(handle.clone()),
                // Under any HUD that lands on this camera.
                GlobalZIndex(i32::MIN),
                UiTargetCamera(camera),
                ScenePresentNode,
            ));
        }
    }
    diag::info!(
        World,
        "render scale {wanted:.2}: 3D view drawn at {}x{}",
        size.x,
        size.y
    );
    scale.current = Some((handle, size));
}

pub(crate) struct RenderScalePlugin;

impl Plugin for RenderScalePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneScale>()
            .add_systems(Update, apply_render_scale);
    }
}
