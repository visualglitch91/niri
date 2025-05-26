// Ported from https://github.com/nferhat/fht-compositor/blob/main/src/renderer/blur/element.rs

use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{GlesError, GlesFrame, GlesRenderer, GlesTexture, Uniform};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::{ContextId, Renderer};
use smithay::output::Output;
use smithay::utils::{Buffer, Logical, Physical, Point, Rectangle, Scale, Size, Transform};

use crate::backend::tty::{TtyFrame, TtyRenderer, TtyRendererError};
use crate::render_helpers::renderer::{AsGlesFrame, NiriRenderer};
use crate::render_helpers::shaders::Shaders;

use super::EffectsFramebuffers;

#[derive(Debug, Clone, PartialEq)]

struct Data {
    loc: Point<f64, Logical>,
    size: Size<f64, Logical>,
    scale: f64,
    noise: f32,
    output: Option<Output>,
    context_id: ContextId<GlesTexture>,
    corner_radius: f32,
}

impl Data {
    fn new(
        loc: Point<f64, Logical>,
        size: Size<f64, Logical>,
        scale: f64,
        noise: f32,
        output: Option<Output>,
        context_id: ContextId<GlesTexture>,
        corner_radius: f32,
    ) -> Self {
        Self {
            loc,
            size,
            scale,
            noise,
            output,
            context_id,
            corner_radius,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BlurRenderElement {
    id: Id,
    data: Data,
    dirty: bool,
    commit_counter: CommitCounter,
}

impl BlurRenderElement {
    pub fn new() -> Self {
        Self {
            id: Id::new(),
            commit_counter: CommitCounter::default(),
            dirty: true,
            data: Data::new(
                Point::from((0.0, 0.0)),
                Size::from((1.0, 1.0)),
                1.0,
                0.0,
                None,
                ContextId::default(),
                0.0,
            ),
        }
    }

    pub fn update(&mut self, size: Size<f64, Logical>, scale: f64) {
        let mut next_data = self.data.clone();
        next_data.size = size;
        next_data.scale = scale;

        if next_data != self.data {
            self.dirty = true;
        } else {
            self.dirty = false;
        }

        self.data = next_data;
        self.commit_counter.increment();
    }

    pub fn with_deps(
        mut self,
        renderer: &mut impl NiriRenderer,
        location: Point<f64, Logical>,
        noise: f32,
        output: Output,
    ) -> Self {
        let mut next_data = self.data.clone();

        next_data.loc = location;
        next_data.noise = noise;
        next_data.output = Some(output);
        next_data.context_id = renderer.as_gles_renderer().context_id();

        if next_data != self.data {
            self.dirty = true;
        }

        self.data = next_data;
        self
    }
}

impl Element for BlurRenderElement {
    fn id(&self) -> &Id {
        &self.id
    }

    fn current_commit(&self) -> CommitCounter {
        self.commit_counter
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        let data = &self.data;
        Rectangle::new(data.loc, data.size).to_buffer(data.scale, Transform::Normal, &data.size)
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        Rectangle::new(
            self.data.loc.to_physical_precise_round(scale),
            self.data.size.to_physical_precise_round(scale),
        )
        .to_i32_up()
    }

    fn location(&self, scale: Scale<f64>) -> Point<i32, Physical> {
        self.geometry(scale).loc
    }

    fn transform(&self) -> Transform {
        Transform::Normal
    }

    fn alpha(&self) -> f32 {
        1.0
    }

    fn kind(&self) -> Kind {
        Kind::Unspecified
    }
}

impl RenderElement<GlesRenderer> for BlurRenderElement {
    fn draw(
        &self,
        gles_frame: &mut GlesFrame,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), GlesError> {
        let data = &self.data;
        let alpha = self.alpha();
        let output_opt = data.output.as_ref();

        if output_opt.is_none() {
            return Ok(());
        }

        let (program, additional_uniforms) = if data.corner_radius == 0.0 {
            (None, vec![])
        } else {
            let program = Shaders::get_from_frame(gles_frame).blur_finish.clone();
            (
                program,
                vec![
                    Uniform::new(
                        "geo",
                        [
                            dst.loc.x as f32,
                            dst.loc.y as f32,
                            dst.size.w as f32,
                            dst.size.h as f32,
                        ],
                    ),
                    Uniform::new("alpha", alpha),
                    Uniform::new("noise", data.noise),
                    Uniform::new("corner_radius", data.corner_radius),
                ],
            )
        };

        let blurred_texture = EffectsFramebuffers::get(output_opt.unwrap())
            .optimized_blur
            .clone();

        let _ = gles_frame.render_texture_from_to(
            &blurred_texture,
            src,
            dst,
            damage,
            opaque_regions,
            Transform::Normal,
            alpha,
            program.as_ref(),
            &additional_uniforms,
        );

        Ok(())
    }

    fn underlying_storage(&self, _: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        None
    }
}

impl<'render> RenderElement<TtyRenderer<'render>> for BlurRenderElement {
    fn draw(
        &self,
        frame: &mut TtyFrame<'_, '_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), TtyRendererError<'render>> {
        <BlurRenderElement as RenderElement<GlesRenderer>>::draw(
            self,
            frame.as_gles_frame(),
            src,
            dst,
            damage,
            opaque_regions,
        )?;

        Ok(())
    }

    fn underlying_storage(
        &self,
        _renderer: &mut TtyRenderer<'render>,
    ) -> Option<UnderlyingStorage> {
        None
    }
}
