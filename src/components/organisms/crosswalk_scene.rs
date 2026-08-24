//! ORGANISM · CrosswalkScene
//!
//! A WebGL2 3D view of a crosswalk graph (see `crate::crosswalk` +
//! `crate::crosswalk3d`): one plane per side, its own classes/attributes/code
//! values as boxes coloured by side, structure/inheritance/association edges
//! as thin neutral parallelepipeds, and the cross-side mapping edges as
//! thicker, brighter parallelepipeds in a third colour.
//!
//! Ports `containers/prez-ui/theme/app/components/ontology/organisms/OntoCrosswalkScene.vue`.
//! That source uses `three` (a JS library, loaded only client-side); this
//! port talks to `WebGl2RenderingContext` directly via `web-sys` instead —
//! no equivalent maintained "port Three.js itself" crate exists, and the
//! scene here is bounded enough (unit-cube boxes, one directional + one
//! ambient light, a damped orbit camera, ray-vs-oriented-box picking) that a
//! small hand-written renderer is more maintainable than binding the whole
//! Three.js API from Rust.
//!
//! Deliberate simplifications from the Vue source (both noted again inline
//! below where they matter):
//! - Lighting is one directional light + a flat ambient term (a simplified
//!   Lambert model), not Three.js's `HemisphereLight` (sky/ground split) +
//!   `DirectionalLight` pair — visually close, much less shader code.
//! - Transparent meshes (edges, which are mostly < 1.0 opacity) are drawn
//!   after opaque meshes (nodes) with depth *test* on but depth *write* off,
//!   in whatever order the graph lists them — not fully back-to-front
//!   depth-sorted. Acceptable for this non-critical visualisation; the Vue
//!   source doesn't sort for translucency either (WebGLRenderer's default
//!   transparent-object ordering is also not a true depth sort).
//! - `OrbitControls`' damping is approximated with a simple per-frame lerp
//!   toward the latest input-driven target (radius/theta/phi), not its exact
//!   internal integration — reads the same to the eye.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glam::{Mat3, Mat4, Vec3};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    Element, HtmlCanvasElement, MouseEvent, ResizeObserver, WebGl2RenderingContext, WebGlBuffer,
    WebGlProgram, WebGlShader, WebGlUniformLocation, WebGlVertexArrayObject, WheelEvent,
};
use yew::prelude::*;

use crate::crosswalk::{edge_color, MatchType, XwalkEdge, XwalkEdgeKind, XwalkGraph, XwalkNode, XwalkTier};
use crate::crosswalk3d::layout_crosswalk_3d;

#[derive(Properties, PartialEq, Clone)]
pub struct CrosswalkSceneProps {
    pub graph: XwalkGraph,
}

// ---- sizing tables (mirrors the Vue source's MATCH_SIZE / KIND_SIZE / nodeSize) --

struct EdgeSize {
    w: f32,
    h: f32,
    opacity: f32,
}

fn edge_size(e: &XwalkEdge) -> EdgeSize {
    if e.kind == XwalkEdgeKind::Mapping {
        match e.matched {
            Some(MatchType::Exact) => EdgeSize { w: 2.0, h: 1.0, opacity: 1.0 },
            Some(MatchType::Close) => EdgeSize { w: 1.3, h: 0.65, opacity: 0.9 },
            Some(MatchType::Related) => EdgeSize { w: 0.8, h: 0.4, opacity: 0.75 },
            None => EdgeSize { w: 0.8, h: 0.4, opacity: 0.75 },
        }
    } else {
        match e.kind {
            XwalkEdgeKind::Structure => EdgeSize { w: 0.5, h: 0.5, opacity: 0.4 },
            XwalkEdgeKind::Inheritance => EdgeSize { w: 1.0, h: 0.5, opacity: 0.75 },
            XwalkEdgeKind::Association => EdgeSize { w: 0.7, h: 0.4, opacity: 0.6 },
            XwalkEdgeKind::Mapping => unreachable!(),
        }
    }
}

fn node_size(n: &XwalkNode) -> f32 {
    match n.tier {
        XwalkTier::Entity => 4.5,
        XwalkTier::Attribute => 2.6,
        XwalkTier::Code => 1.5,
    }
}

fn edge_key(e: &XwalkEdge) -> String {
    format!("{:?}|{}|{}|{}", e.kind, e.source, e.target, e.matched.map(|m| m.label()).unwrap_or(""))
}

fn hex_to_rgb(hex: &str) -> [f32; 3] {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return [0.6, 0.6, 0.6];
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(128) as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(128) as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(128) as f32 / 255.0;
    [r, g, b]
}

// ---- filtering (mirrors the Vue source's `visibleGraph` computed) -----------

/// Re-derive a fresh, filtered, freshly-laid-out sub-graph rather than just
/// hiding meshes — otherwise the rings keep the gaps their hidden siblings
/// left and the whole point (a compact, legible arrangement) is lost.
fn visible_graph(graph: &XwalkGraph, show_code_values: bool, mapped_only: bool) -> XwalkGraph {
    let mut nodes: Vec<XwalkNode> = graph.nodes.clone();
    if !show_code_values {
        nodes.retain(|n| n.tier != XwalkTier::Code);
    }
    if mapped_only {
        let mut keep: std::collections::HashSet<String> = nodes.iter().filter(|n| n.match_count > 0).map(|n| n.iri.clone()).collect();
        // Keep each mapped node's ancestor chain too, or it renders as a
        // disconnected dot with no visible route back to its entity.
        let by_iri: HashMap<String, XwalkNode> = nodes.iter().map(|n| (n.iri.clone(), n.clone())).collect();
        for iri in keep.clone() {
            let mut cur = by_iri.get(&iri).and_then(|n| n.parent.clone());
            while let Some(c) = cur {
                if !keep.insert(c.clone()) {
                    break;
                }
                cur = by_iri.get(&c).and_then(|n| n.parent.clone());
            }
        }
        nodes.retain(|n| keep.contains(&n.iri));
    }
    let node_iris: std::collections::HashSet<String> = nodes.iter().map(|n| n.iri.clone()).collect();
    for n in &mut nodes {
        n.children.retain(|c| node_iris.contains(c));
        if let Some(p) = &n.parent {
            if !node_iris.contains(p) {
                n.parent = None;
            }
        }
    }
    let edges: Vec<XwalkEdge> = graph.edges.iter().filter(|e| node_iris.contains(&e.source) && node_iris.contains(&e.target)).cloned().collect();
    let sides = graph
        .sides
        .iter()
        .map(|s| crate::crosswalk::XwalkSide { node_count: nodes.iter().filter(|n| n.side == s.slug).count(), ..s.clone() })
        .filter(|s| s.node_count > 0)
        .collect();
    let mut g = XwalkGraph { nodes, edges, sides, stats: graph.stats };
    layout_crosswalk_3d(&mut g);
    g
}

// ---- WebGL plumbing -----------------------------------------------------

fn compile_shader(gl: &WebGl2RenderingContext, kind: u32, src: &str) -> Result<WebGlShader, String> {
    let shader = gl.create_shader(kind).ok_or("create_shader failed")?;
    gl.shader_source(&shader, src);
    gl.compile_shader(&shader);
    if gl.get_shader_parameter(&shader, WebGl2RenderingContext::COMPILE_STATUS).as_bool().unwrap_or(false) {
        Ok(shader)
    } else {
        Err(gl.get_shader_info_log(&shader).unwrap_or_else(|| "unknown shader error".into()))
    }
}

fn link_program(gl: &WebGl2RenderingContext, vs_src: &str, fs_src: &str) -> Result<WebGlProgram, String> {
    let vs = compile_shader(gl, WebGl2RenderingContext::VERTEX_SHADER, vs_src)?;
    let fs = compile_shader(gl, WebGl2RenderingContext::FRAGMENT_SHADER, fs_src)?;
    let program = gl.create_program().ok_or("create_program failed")?;
    gl.attach_shader(&program, &vs);
    gl.attach_shader(&program, &fs);
    gl.link_program(&program);
    if gl.get_program_parameter(&program, WebGl2RenderingContext::LINK_STATUS).as_bool().unwrap_or(false) {
        Ok(program)
    } else {
        Err(gl.get_program_info_log(&program).unwrap_or_else(|| "unknown link error".into()))
    }
}

const VERTEX_SRC: &str = r#"#version 300 es
layout(location = 0) in vec3 a_position;
layout(location = 1) in vec3 a_normal;
uniform mat4 u_model;
uniform mat4 u_view;
uniform mat4 u_proj;
uniform mat3 u_normalMatrix;
out vec3 v_normal;
void main() {
    v_normal = u_normalMatrix * a_normal;
    gl_Position = u_proj * u_view * u_model * vec4(a_position, 1.0);
}
"#;

// Simplified Lambert model (ambient + one directional light) standing in for
// the Vue source's HemisphereLight + DirectionalLight pair — see the module
// doc comment.
const FRAGMENT_SRC: &str = r#"#version 300 es
precision mediump float;
in vec3 v_normal;
uniform vec3 u_color;
uniform float u_opacity;
uniform vec3 u_lightDir;
out vec4 outColor;
void main() {
    vec3 n = normalize(v_normal);
    float diff = max(dot(n, u_lightDir), 0.0);
    vec3 ambient = vec3(0.45);
    vec3 color = u_color * clamp(ambient + diff * 0.65, 0.0, 1.0);
    outColor = vec4(color, u_opacity);
}
"#;

/// A unit cube (`[-0.5, 0.5]^3`), 24 vertices (4 per face, flat-shaded via a
/// per-face normal) interleaved as `[x,y,z, nx,ny,nz] * 24`, drawn with the
/// matching 36-index triangle list below.
#[rustfmt::skip]
const CUBE_VERTICES: [f32; 24 * 6] = [
    // +X
     0.5,-0.5,-0.5,  1.0,0.0,0.0,   0.5, 0.5,-0.5,  1.0,0.0,0.0,   0.5, 0.5, 0.5,  1.0,0.0,0.0,   0.5,-0.5, 0.5,  1.0,0.0,0.0,
    // -X
    -0.5,-0.5, 0.5, -1.0,0.0,0.0,  -0.5, 0.5, 0.5, -1.0,0.0,0.0,  -0.5, 0.5,-0.5, -1.0,0.0,0.0,  -0.5,-0.5,-0.5, -1.0,0.0,0.0,
    // +Y
    -0.5, 0.5,-0.5,  0.0,1.0,0.0,  -0.5, 0.5, 0.5,  0.0,1.0,0.0,   0.5, 0.5, 0.5,  0.0,1.0,0.0,   0.5, 0.5,-0.5,  0.0,1.0,0.0,
    // -Y
    -0.5,-0.5, 0.5,  0.0,-1.0,0.0, -0.5,-0.5,-0.5,  0.0,-1.0,0.0,  0.5,-0.5,-0.5,  0.0,-1.0,0.0,   0.5,-0.5, 0.5,  0.0,-1.0,0.0,
    // +Z
    -0.5,-0.5, 0.5,  0.0,0.0,1.0,   0.5,-0.5, 0.5,  0.0,0.0,1.0,   0.5, 0.5, 0.5,  0.0,0.0,1.0,  -0.5, 0.5, 0.5,  0.0,0.0,1.0,
    // -Z
     0.5,-0.5,-0.5,  0.0,0.0,-1.0, -0.5,-0.5,-0.5,  0.0,0.0,-1.0, -0.5, 0.5,-0.5,  0.0,0.0,-1.0,   0.5, 0.5,-0.5,  0.0,0.0,-1.0,
];

#[rustfmt::skip]
const CUBE_INDICES: [u16; 36] = [
    0,1,2, 0,2,3,       // +X
    4,5,6, 4,6,7,       // -X
    8,9,10, 8,10,11,    // +Y
    12,13,14, 12,14,15, // -Y
    16,17,18, 16,18,19, // +Z
    20,21,22, 20,22,23, // -Z
];

/// A single drawable box: a node or an edge, identified so hover/selection
/// state can be looked back up against `XwalkGraph` data.
#[derive(Clone)]
enum MeshId {
    Node(String),
    Edge(String),
}

#[derive(Clone)]
struct SceneMesh {
    id: MeshId,
    model: Mat4,
    color: [f32; 3],
    opacity: f32,
}

/// Orbit camera around a fixed target at the origin — spherical coordinates,
/// same convention Three.js's `OrbitControls` uses internally (`theta`:
/// azimuth around +Y measured from +Z; `phi`: polar angle from +Y).
struct OrbitCamera {
    radius: f32,
    theta: f32,
    phi: f32,
    radius_target: f32,
    theta_target: f32,
    phi_target: f32,
}

impl OrbitCamera {
    fn new() -> Self {
        let start = Vec3::new(150.0, 120.0, 200.0);
        let radius = start.length();
        let theta = start.x.atan2(start.z);
        let phi = (start.y / radius).acos();
        OrbitCamera { radius, theta, phi, radius_target: radius, theta_target: theta, phi_target: phi }
    }

    fn damp(&mut self) {
        const F: f32 = 0.08;
        self.radius += (self.radius_target - self.radius) * F;
        self.theta += (self.theta_target - self.theta) * F;
        self.phi += (self.phi_target - self.phi) * F;
    }

    fn eye(&self) -> Vec3 {
        Vec3::new(self.radius * self.phi.sin() * self.theta.sin(), self.radius * self.phi.cos(), self.radius * self.phi.sin() * self.theta.cos())
    }

    fn view(&self) -> Mat4 {
        glam::camera::rh::view::look_at_mat4(self.eye(), Vec3::ZERO, Vec3::Y)
    }
}

/// Everything that must survive across renders and across the RAF loop's own
/// closure invocations — held behind `Rc<RefCell<_>>` so DOM event listeners,
/// the mount effect, and the data-driven update effect can all reach it.
struct Scene {
    gl: WebGl2RenderingContext,
    canvas: HtmlCanvasElement,
    program: WebGlProgram,
    vao: WebGlVertexArrayObject,
    u_model: WebGlUniformLocation,
    u_view: WebGlUniformLocation,
    u_proj: WebGlUniformLocation,
    u_normal_matrix: WebGlUniformLocation,
    u_color: WebGlUniformLocation,
    u_opacity: WebGlUniformLocation,
    u_light_dir: WebGlUniformLocation,
    node_meshes: Vec<SceneMesh>,
    edge_meshes: Vec<SceneMesh>,
    camera: OrbitCamera,
    dragging: bool,
    last_pointer: (f32, f32),
    selected_iri: Option<String>,
    hovered_iri: Option<String>,
    hovered_edge_key: Option<String>,
}

const MIN_DISTANCE: f32 = 40.0;
const MAX_DISTANCE: f32 = 700.0;

impl Scene {
    fn opacity_for(&self, id: &MeshId, base: f32) -> f32 {
        match id {
            MeshId::Node(iri) => {
                if Some(iri.as_str()) == self.selected_iri.as_deref() || Some(iri.as_str()) == self.hovered_iri.as_deref() {
                    1.0
                } else {
                    0.92
                }
            }
            MeshId::Edge(key) => {
                if Some(key.as_str()) == self.hovered_edge_key.as_deref() {
                    1.0
                } else {
                    base
                }
            }
        }
    }

    fn draw(&self) {
        let gl = &self.gl;
        let width = self.canvas.width().max(1) as f32;
        let height = self.canvas.height().max(1) as f32;
        gl.viewport(0, 0, width as i32, height as i32);
        gl.clear_color(0.043, 0.063, 0.126, 1.0); // #0b1020, matches the Vue source's scene background
        gl.clear(WebGl2RenderingContext::COLOR_BUFFER_BIT | WebGl2RenderingContext::DEPTH_BUFFER_BIT);
        gl.enable(WebGl2RenderingContext::DEPTH_TEST);
        gl.enable(WebGl2RenderingContext::BLEND);
        gl.blend_func(WebGl2RenderingContext::SRC_ALPHA, WebGl2RenderingContext::ONE_MINUS_SRC_ALPHA);

        gl.use_program(Some(&self.program));
        gl.bind_vertex_array(Some(&self.vao));

        let proj = glam::camera::rh::proj::opengl::perspective(50f32.to_radians(), width / height, 1.0, 2000.0);
        let view = self.camera.view();
        let light_dir = Vec3::new(1.0, 1.2, 0.6).normalize();
        gl.uniform_matrix4fv_with_f32_array(Some(&self.u_proj), false, &proj.to_cols_array());
        gl.uniform_matrix4fv_with_f32_array(Some(&self.u_view), false, &view.to_cols_array());
        gl.uniform3f(Some(&self.u_light_dir), light_dir.x, light_dir.y, light_dir.z);

        // Opaque nodes first, depth write on.
        gl.depth_mask(true);
        for m in &self.node_meshes {
            self.draw_mesh(m, self.opacity_for(&m.id, m.opacity));
        }
        // Translucent edges after, depth write off (see module doc comment).
        gl.depth_mask(false);
        for m in &self.edge_meshes {
            let opacity = self.opacity_for(&m.id, m.opacity);
            if opacity <= 0.0 {
                continue;
            }
            self.draw_mesh(m, opacity);
        }
        gl.depth_mask(true);
    }

    fn draw_mesh(&self, m: &SceneMesh, opacity: f32) {
        let gl = &self.gl;
        let normal_matrix = Mat3::from_mat4(m.model).inverse().transpose();
        gl.uniform_matrix4fv_with_f32_array(Some(&self.u_model), false, &m.model.to_cols_array());
        gl.uniform_matrix3fv_with_f32_array(Some(&self.u_normal_matrix), false, &normal_matrix.to_cols_array());
        gl.uniform3f(Some(&self.u_color), m.color[0], m.color[1], m.color[2]);
        gl.uniform1f(Some(&self.u_opacity), opacity);
        gl.draw_elements_with_i32(WebGl2RenderingContext::TRIANGLES, 36, WebGl2RenderingContext::UNSIGNED_SHORT, 0);
    }

    /// Rebuild the mesh list from a freshly filtered+laid-out graph — mirrors
    /// the Vue source's `rebuild()`.
    fn rebuild(&mut self, graph: &XwalkGraph) {
        let pos: HashMap<&str, Vec3> = graph.nodes.iter().map(|n| (n.iri.as_str(), Vec3::new(n.x as f32, n.y as f32, n.z as f32))).collect();

        self.node_meshes = graph
            .nodes
            .iter()
            .map(|n| {
                let size = node_size(n);
                let model = Mat4::from_scale_rotation_translation(Vec3::splat(size), glam::Quat::IDENTITY, pos[n.iri.as_str()]);
                SceneMesh { id: MeshId::Node(n.iri.clone()), model, color: hex_to_rgb(&n.color), opacity: 0.92 }
            })
            .collect();

        self.edge_meshes = graph
            .edges
            .iter()
            .filter_map(|e| {
                let a = *pos.get(e.source.as_str())?;
                let b = *pos.get(e.target.as_str())?;
                let size = edge_size(e);
                orient_box(a, b, size.w, size.h).map(|model| SceneMesh {
                    id: MeshId::Edge(edge_key(e)),
                    model,
                    color: hex_to_rgb(edge_color(e)),
                    opacity: size.opacity,
                })
            })
            .collect();
    }

    /// World-space ray from the camera through a canvas-relative point, for
    /// picking — mirrors `Raycaster.setFromCamera`.
    fn pick_ray(&self, ndc_x: f32, ndc_y: f32) -> (Vec3, Vec3) {
        let width = self.canvas.width().max(1) as f32;
        let height = self.canvas.height().max(1) as f32;
        let proj = glam::camera::rh::proj::opengl::perspective(50f32.to_radians(), width / height, 1.0, 2000.0);
        let view = self.camera.view();
        let inv = (proj * view).inverse();
        let near = inv.project_point3(Vec3::new(ndc_x, ndc_y, -1.0));
        let far = inv.project_point3(Vec3::new(ndc_x, ndc_y, 1.0));
        (near, (far - near).normalize())
    }

    /// Nearest ray-vs-oriented-box hit among `meshes`, by transforming the
    /// (already unit-length-direction) world ray into each mesh's local
    /// `[-0.5,0.5]^3` space via its inverse model matrix — the resulting
    /// slab-test `t` is directly comparable in world units across meshes
    /// because it's preserved by any invertible affine transform applied
    /// consistently to both the ray origin and (unnormalised) direction.
    fn pick<'a>(origin: Vec3, dir: Vec3, meshes: &'a [SceneMesh]) -> Option<&'a MeshId> {
        let mut best: Option<(f32, &MeshId)> = None;
        for m in meshes {
            let inv = m.model.inverse();
            let lo = inv.transform_point3(origin);
            let ld = inv.transform_vector3(dir);
            if let Some(t) = ray_aabb(lo, ld) {
                if t >= 0.0 && best.map(|(bt, _)| t < bt).unwrap_or(true) {
                    best = Some((t, &m.id));
                }
            }
        }
        best.map(|(_, id)| id)
    }
}

fn ray_aabb(origin: Vec3, dir: Vec3) -> Option<f32> {
    let mut tmin = f32::NEG_INFINITY;
    let mut tmax = f32::INFINITY;
    for axis in 0..3 {
        let o = origin[axis];
        let d = dir[axis];
        if d.abs() < 1e-9 {
            if !(-0.5..=0.5).contains(&o) {
                return None;
            }
        } else {
            let mut t1 = (-0.5 - o) / d;
            let mut t2 = (0.5 - o) / d;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        }
    }
    if tmax < 0.0 {
        return None;
    }
    Some(if tmin >= 0.0 { tmin } else { tmax })
}

/// Orient+scale+position a unit cube so it spans from `a` to `b` with cross-
/// section `w`x`h` — mirrors the Vue source's `orientBox`. Vertical bridges
/// (near-parallel to world-up) are exactly where `cross(worldUp, dir)`
/// degenerates, so fall back to a second reference axis, same as the source.
fn orient_box(a: Vec3, b: Vec3, w: f32, h: f32) -> Option<Mat4> {
    let dir_vec = b - a;
    let len = dir_vec.length();
    if len < 1e-6 {
        return None;
    }
    let dir = dir_vec / len;
    let position = (a + b) * 0.5;
    let up = if dir.y.abs() > 0.99 { Vec3::X } else { Vec3::Y };
    let x_axis = up.cross(dir).normalize();
    let y_axis = dir.cross(x_axis);
    let rotation = glam::Quat::from_mat3(&Mat3::from_cols(x_axis, y_axis, dir));
    Some(Mat4::from_scale_rotation_translation(Vec3::new(w, h, len), rotation, position))
}

// ---- the Yew component ---------------------------------------------------

const EDGE_KIND_LABEL: [(XwalkEdgeKind, &str); 4] = [
    (XwalkEdgeKind::Structure, "Structure (broader / narrower)"),
    (XwalkEdgeKind::Inheritance, "Generalization (subClassOf)"),
    (XwalkEdgeKind::Association, "Association (object property)"),
    (XwalkEdgeKind::Mapping, "Mapping"),
];

fn edge_kind_label(kind: XwalkEdgeKind) -> &'static str {
    EDGE_KIND_LABEL.iter().find(|(k, _)| *k == kind).map(|(_, l)| *l).unwrap_or("")
}

struct Tooltip {
    title: String,
    curie: Option<String>,
    body: Option<String>,
}

#[function_component(CrosswalkScene)]
pub fn crosswalk_scene(props: &CrosswalkSceneProps) -> Html {
    let show_code_values = use_state(|| false);
    let mapped_only = use_state(|| false);
    let selected_iri = use_state(|| None::<String>);
    let hovered_iri = use_state(|| None::<String>);
    let hovered_edge_key = use_state(|| None::<String>);
    let pointer_screen = use_state(|| None::<(f64, f64)>);
    let webgl_supported = use_state(|| true);
    let scene: UseStateHandle<Rc<RefCell<Option<Scene>>>> = use_state(|| Rc::new(RefCell::new(None)));
    let canvas_ref = use_node_ref();
    let host_ref = use_node_ref();

    let visible = use_memo((props.graph.clone(), *show_code_values, *mapped_only), |(g, show_code, mapped)| {
        visible_graph(g, *show_code, *mapped)
    });

    let by_iri: HashMap<&str, &XwalkNode> = visible.nodes.iter().map(|n| (n.iri.as_str(), n)).collect();
    let selected_node = selected_iri.as_deref().and_then(|iri| by_iri.get(iri)).copied();
    let selected_mappings: Vec<(&XwalkEdge, Option<&XwalkNode>)> = selected_node
        .map(|n| {
            visible
                .edges
                .iter()
                .filter(|e| e.kind == XwalkEdgeKind::Mapping && (e.source == n.iri || e.target == n.iri))
                .map(|e| {
                    let partner_iri = if e.source == n.iri { &e.target } else { &e.source };
                    (e, by_iri.get(partner_iri.as_str()).copied())
                })
                .collect()
        })
        .unwrap_or_default();

    let hovered_edge = hovered_edge_key.as_deref().and_then(|k| visible.edges.iter().find(|e| edge_key(e) == k));
    let tooltip: Option<Tooltip> = if let Some(iri) = hovered_iri.as_deref() {
        by_iri.get(iri).map(|n| Tooltip { title: n.label.clone(), curie: Some(n.curie.clone()), body: n.description.clone() })
    } else {
        hovered_edge.map(|e| {
            let s = by_iri.get(e.source.as_str());
            let t = by_iri.get(e.target.as_str());
            let title = format!("{} \u{2192} {}", s.map(|n| n.label.as_str()).unwrap_or(&e.source), t.map(|n| n.label.as_str()).unwrap_or(&e.target));
            let body = if e.kind == XwalkEdgeKind::Mapping {
                format!("{}Match{}", e.matched.map(|m| m.label()).unwrap_or(""), e.rationale.as_deref().map(|r| format!(" \u{2014} {r}")).unwrap_or_default())
            } else {
                edge_kind_label(e.kind).to_string()
            };
            Tooltip { title, curie: None, body: Some(body) }
        })
    };

    // Mount: check WebGL2 support, then set up the GL context + render loop
    // once. Everything mutable afterwards lives in `scene` (Rc<RefCell<_>>)
    // so the effects below and the DOM event closures can all reach it.
    {
        let scene = scene.clone();
        let canvas_ref = canvas_ref.clone();
        let host_ref = host_ref.clone();
        let webgl_supported = webgl_supported.clone();
        let hovered_iri = hovered_iri.clone();
        let hovered_edge_key = hovered_edge_key.clone();
        let selected_iri_handle = selected_iri.clone();
        let pointer_screen = pointer_screen.clone();
        use_effect_with((), move |_| {
            let cleanup: Box<dyn FnOnce()> = (|| {
                let Some(canvas_el) = canvas_ref.cast::<HtmlCanvasElement>() else { return Box::new(|| ()) as Box<dyn FnOnce()> };
                let Some(host_el) = host_ref.cast::<Element>() else { return Box::new(|| ()) as Box<dyn FnOnce()> };
                let Ok(Some(gl)) = canvas_el.get_context("webgl2") else {
                    webgl_supported.set(false);
                    return Box::new(|| ()) as Box<dyn FnOnce()>;
                };
                let Ok(gl) = gl.dyn_into::<WebGl2RenderingContext>() else {
                    webgl_supported.set(false);
                    return Box::new(|| ()) as Box<dyn FnOnce()>;
                };

                let program = match link_program(&gl, VERTEX_SRC, FRAGMENT_SRC) {
                    Ok(p) => p,
                    Err(err) => {
                        log::error!("crosswalk scene shader link failed: {err}");
                        webgl_supported.set(false);
                        return Box::new(|| ()) as Box<dyn FnOnce()>;
                    }
                };

                let vao = gl.create_vertex_array().expect("create_vertex_array");
                gl.bind_vertex_array(Some(&vao));

                let vbo: WebGlBuffer = gl.create_buffer().expect("create_buffer vbo");
                gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&vbo));
                unsafe {
                    let view = js_sys::Float32Array::view(&CUBE_VERTICES);
                    gl.buffer_data_with_array_buffer_view(WebGl2RenderingContext::ARRAY_BUFFER, &view, WebGl2RenderingContext::STATIC_DRAW);
                }
                let stride = 6 * 4;
                gl.vertex_attrib_pointer_with_i32(0, 3, WebGl2RenderingContext::FLOAT, false, stride, 0);
                gl.enable_vertex_attrib_array(0);
                gl.vertex_attrib_pointer_with_i32(1, 3, WebGl2RenderingContext::FLOAT, false, stride, 3 * 4);
                gl.enable_vertex_attrib_array(1);

                let ebo: WebGlBuffer = gl.create_buffer().expect("create_buffer ebo");
                gl.bind_buffer(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, Some(&ebo));
                unsafe {
                    let view = js_sys::Uint16Array::view(&CUBE_INDICES);
                    gl.buffer_data_with_array_buffer_view(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, &view, WebGl2RenderingContext::STATIC_DRAW);
                }

                let u = |name: &str| gl.get_uniform_location(&program, name).unwrap_or_else(|| panic!("uniform {name} not found"));
                let u_model = u("u_model");
                let u_view = u("u_view");
                let u_proj = u("u_proj");
                let u_normal_matrix = u("u_normalMatrix");
                let u_color = u("u_color");
                let u_opacity = u("u_opacity");
                let u_light_dir = u("u_lightDir");
                let new_scene = Scene {
                    gl: gl.clone(),
                    canvas: canvas_el.clone(),
                    program,
                    vao,
                    u_model,
                    u_view,
                    u_proj,
                    u_normal_matrix,
                    u_color,
                    u_opacity,
                    u_light_dir,
                    node_meshes: Vec::new(),
                    edge_meshes: Vec::new(),
                    camera: OrbitCamera::new(),
                    dragging: false,
                    last_pointer: (0.0, 0.0),
                    selected_iri: None,
                    hovered_iri: None,
                    hovered_edge_key: None,
                };
                *scene.borrow_mut() = Some(new_scene);

                // -- resize --------------------------------------------------
                let resize_scene = scene.clone();
                let resize_host = host_el.clone();
                let resize_canvas = canvas_el.clone();
                let resize_closure = Closure::<dyn Fn()>::new(move || {
                    let w = resize_host.client_width().max(1) as u32;
                    let h = resize_host.client_height().max(1) as u32;
                    resize_canvas.set_width(w);
                    resize_canvas.set_height(h);
                    if let Some(s) = resize_scene.borrow().as_ref() {
                        s.draw();
                    }
                });
                let resize_observer = ResizeObserver::new(resize_closure.as_ref().unchecked_ref()).ok();
                if let Some(ro) = &resize_observer {
                    ro.observe(&host_el);
                }
                // Prime the initial size before the first RAF tick.
                canvas_el.set_width(host_el.client_width().max(1) as u32);
                canvas_el.set_height(host_el.client_height().max(1) as u32);

                // -- pointer / wheel: orbit + picking -------------------------
                let ndc = |el: &HtmlCanvasElement, client_x: f32, client_y: f32| -> (f32, f32) {
                    let rect = el.get_bounding_client_rect();
                    let x = ((client_x - rect.left() as f32) / rect.width() as f32) * 2.0 - 1.0;
                    let y = -((client_y - rect.top() as f32) / rect.height() as f32) * 2.0 + 1.0;
                    (x, y)
                };

                let down_scene = scene.clone();
                let onpointerdown = Closure::<dyn Fn(MouseEvent)>::new(move |e: MouseEvent| {
                    if let Some(s) = down_scene.borrow_mut().as_mut() {
                        s.dragging = true;
                        s.last_pointer = (e.client_x() as f32, e.client_y() as f32);
                    }
                });
                canvas_el.add_event_listener_with_callback("pointerdown", onpointerdown.as_ref().unchecked_ref()).ok();

                let move_scene = scene.clone();
                let move_canvas = canvas_el.clone();
                let move_hovered_iri = hovered_iri.clone();
                let move_hovered_edge_key = hovered_edge_key.clone();
                let move_pointer_screen = pointer_screen.clone();
                let onpointermove = Closure::<dyn Fn(MouseEvent)>::new(move |e: MouseEvent| {
                    let rect = move_canvas.get_bounding_client_rect();
                    move_pointer_screen.set(Some((e.client_x() as f64 - rect.left(), e.client_y() as f64 - rect.top())));
                    let mut scene_ref = move_scene.borrow_mut();
                    let Some(s) = scene_ref.as_mut() else { return };
                    if s.dragging {
                        const ROTATE_SPEED: f32 = 0.008;
                        let (lx, ly) = s.last_pointer;
                        let (cx, cy) = (e.client_x() as f32, e.client_y() as f32);
                        s.camera.theta_target -= (cx - lx) * ROTATE_SPEED;
                        s.camera.phi_target = (s.camera.phi_target - (cy - ly) * ROTATE_SPEED).clamp(0.05, std::f32::consts::PI - 0.05);
                        s.last_pointer = (cx, cy);
                        return;
                    }
                    let (nx, ny) = ndc(&move_canvas, e.client_x() as f32, e.client_y() as f32);
                    let (origin, dir) = s.pick_ray(nx, ny);
                    let node_hit = Scene::pick(origin, dir, &s.node_meshes);
                    let (hovered_node, hovered_edge) = match node_hit {
                        Some(MeshId::Node(iri)) => (Some(iri.clone()), None),
                        _ => match Scene::pick(origin, dir, &s.edge_meshes) {
                            Some(MeshId::Edge(key)) => (None, Some(key.clone())),
                            _ => (None, None),
                        },
                    };
                    if s.hovered_iri != hovered_node {
                        s.hovered_iri = hovered_node.clone();
                        move_hovered_iri.set(hovered_node);
                    }
                    if s.hovered_edge_key != hovered_edge {
                        s.hovered_edge_key = hovered_edge.clone();
                        move_hovered_edge_key.set(hovered_edge);
                    }
                });
                canvas_el.add_event_listener_with_callback("pointermove", onpointermove.as_ref().unchecked_ref()).ok();

                let up_scene = scene.clone();
                let leave_hovered_iri = hovered_iri.clone();
                let leave_hovered_edge_key = hovered_edge_key.clone();
                let leave_pointer_screen = pointer_screen.clone();
                let onpointerup_leave = Closure::<dyn Fn(MouseEvent)>::new(move |_: MouseEvent| {
                    if let Some(s) = up_scene.borrow_mut().as_mut() {
                        s.dragging = false;
                    }
                    leave_hovered_iri.set(None);
                    leave_hovered_edge_key.set(None);
                    leave_pointer_screen.set(None);
                });
                canvas_el.add_event_listener_with_callback("pointerup", onpointerup_leave.as_ref().unchecked_ref()).ok();
                canvas_el.add_event_listener_with_callback("pointerleave", onpointerup_leave.as_ref().unchecked_ref()).ok();

                let click_scene = scene.clone();
                let click_canvas = canvas_el.clone();
                let click_selected = selected_iri_handle.clone();
                let onclick = Closure::<dyn Fn(MouseEvent)>::new(move |e: MouseEvent| {
                    let (nx, ny) = ndc(&click_canvas, e.client_x() as f32, e.client_y() as f32);
                    let scene_ref = click_scene.borrow();
                    let Some(s) = scene_ref.as_ref() else { return };
                    let (origin, dir) = s.pick_ray(nx, ny);
                    if let Some(MeshId::Node(iri)) = Scene::pick(origin, dir, &s.node_meshes) {
                        let iri = iri.clone();
                        drop(scene_ref);
                        click_selected.set(if click_selected.as_deref() == Some(iri.as_str()) { None } else { Some(iri) });
                    } else {
                        drop(scene_ref);
                        click_selected.set(None);
                    }
                });
                canvas_el.add_event_listener_with_callback("click", onclick.as_ref().unchecked_ref()).ok();

                let wheel_scene = scene.clone();
                let onwheel = Closure::<dyn Fn(WheelEvent)>::new(move |e: WheelEvent| {
                    e.prevent_default();
                    if let Some(s) = wheel_scene.borrow_mut().as_mut() {
                        let factor = if e.delta_y() > 0.0 { 1.1 } else { 1.0 / 1.1 };
                        s.camera.radius_target = (s.camera.radius_target * factor).clamp(MIN_DISTANCE, MAX_DISTANCE);
                    }
                });
                canvas_el.add_event_listener_with_callback("wheel", onwheel.as_ref().unchecked_ref()).ok();

                // -- render loop ----------------------------------------------
                let raf_id = Rc::new(RefCell::new(0i32));
                let raf_scene = scene.clone();
                let raf_closure: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
                let raf_closure_slot = raf_closure.clone();
                let raf_id_for_tick = raf_id.clone();
                *raf_closure.borrow_mut() = Some(Closure::wrap(Box::new(move || {
                    if let Some(s) = raf_scene.borrow_mut().as_mut() {
                        s.camera.damp();
                        s.draw();
                    }
                    if let Some(win) = web_sys::window() {
                        if let Some(f) = raf_closure_slot.borrow().as_ref() {
                            if let Ok(id) = win.request_animation_frame(f.as_ref().unchecked_ref()) {
                                *raf_id_for_tick.borrow_mut() = id;
                            }
                        }
                    }
                }) as Box<dyn FnMut()>));
                if let Some(win) = web_sys::window() {
                    if let Some(f) = raf_closure.borrow().as_ref() {
                        if let Ok(id) = win.request_animation_frame(f.as_ref().unchecked_ref()) {
                            *raf_id.borrow_mut() = id;
                        }
                    }
                }

                let cleanup_canvas = canvas_el.clone();
                Box::new(move || {
                    if let Some(win) = web_sys::window() {
                        win.cancel_animation_frame(*raf_id.borrow()).ok();
                    }
                    if let Some(ro) = resize_observer {
                        ro.disconnect();
                    }
                    let _ = &resize_closure; // kept alive until here
                    let _ = &raf_closure;
                    cleanup_canvas.remove_event_listener_with_callback("pointerdown", onpointerdown.as_ref().unchecked_ref()).ok();
                    cleanup_canvas.remove_event_listener_with_callback("pointermove", onpointermove.as_ref().unchecked_ref()).ok();
                    cleanup_canvas.remove_event_listener_with_callback("pointerup", onpointerup_leave.as_ref().unchecked_ref()).ok();
                    cleanup_canvas.remove_event_listener_with_callback("pointerleave", onpointerup_leave.as_ref().unchecked_ref()).ok();
                    cleanup_canvas.remove_event_listener_with_callback("click", onclick.as_ref().unchecked_ref()).ok();
                    cleanup_canvas.remove_event_listener_with_callback("wheel", onwheel.as_ref().unchecked_ref()).ok();
                })
            })();
            cleanup
        });
    }

    // Rebuild the mesh list whenever the filtered/laid-out graph changes.
    {
        let scene = scene.clone();
        let graph_for_effect = (*visible).clone();
        use_effect_with(graph_for_effect, move |g| {
            if let Some(s) = scene.borrow_mut().as_mut() {
                s.rebuild(g);
            }
            || ()
        });
    }

    // Push selection/hover state into the scene so the render loop picks up
    // the new highlight colours on its next frame.
    {
        let scene = scene.clone();
        let selected = (*selected_iri).clone();
        let hovered = (*hovered_iri).clone();
        let hovered_edge = (*hovered_edge_key).clone();
        use_effect_with((selected.clone(), hovered.clone(), hovered_edge.clone()), move |(sel, hov, hov_edge)| {
            if let Some(s) = scene.borrow_mut().as_mut() {
                s.selected_iri = sel.clone();
                s.hovered_iri = hov.clone();
                s.hovered_edge_key = hov_edge.clone();
            }
            || ()
        });
    }

    let toggle = |flag: UseStateHandle<bool>| Callback::from(move |_: MouseEvent| flag.set(!*flag));

    html! {
        <div class="crosswalk-scene">
            <div class="crosswalk-scene__legend">
                { for props.graph.sides.iter().map(|s| html! {
                    <span key={s.slug.clone()} class="crosswalk-scene__legend-item">
                        <span class="crosswalk-scene__swatch" style={format!("background-color:{}", s.color)} aria-hidden="true" />
                        { s.title.clone() }
                        { " (" }{ s.node_count }{ ")" }
                    </span>
                }) }
                <span class="crosswalk-scene__legend-item">
                    <span class="crosswalk-scene__swatch crosswalk-scene__swatch--mapping" aria-hidden="true" />
                    { format!("mapping (exact {} \u{b7} close {} \u{b7} related {})", props.graph.stats.by_match.exact, props.graph.stats.by_match.close, props.graph.stats.by_match.related) }
                </span>
                <span class="crosswalk-scene__legend-item">
                    <span class="crosswalk-scene__swatch crosswalk-scene__swatch--edge" aria-hidden="true" />
                    { "structure / association" }
                </span>
                <span class="crosswalk-scene__coverage">
                    { format!("{} of {} concepts mapped ({}%)", props.graph.stats.mapped, props.graph.stats.nodes, if props.graph.stats.nodes > 0 { props.graph.stats.mapped * 100 / props.graph.stats.nodes } else { 0 }) }
                </span>
            </div>

            <div class="crosswalk-scene__controls">
                <label class="crosswalk-scene__switch-label">
                    <button
                        type="button" role="switch" aria-checked={show_code_values.to_string()}
                        class={classes!("crosswalk-scene__switch", (*show_code_values).then_some("crosswalk-scene__switch--on"))}
                        onclick={toggle(show_code_values.clone())}
                    >
                        <span class="crosswalk-scene__switch-knob" />
                    </button>
                    { "Code values" }
                </label>
                <label class="crosswalk-scene__switch-label">
                    <button
                        type="button" role="switch" aria-checked={mapped_only.to_string()}
                        class={classes!("crosswalk-scene__switch", (*mapped_only).then_some("crosswalk-scene__switch--on"))}
                        onclick={toggle(mapped_only.clone())}
                    >
                        <span class="crosswalk-scene__switch-knob" />
                    </button>
                    { "Mapped only" }
                </label>
                <span class="crosswalk-scene__hint">{ "Drag to orbit \u{b7} scroll to zoom \u{b7} hover for a summary \u{b7} click a box for details" }</span>
            </div>

            <div class="crosswalk-scene__canvas-wrap" ref={host_ref}>
                if !*webgl_supported {
                    <div class="crosswalk-scene__no-webgl">
                        { "WebGL isn't available in this browser \u{2014} see the mapping table below for the same data." }
                    </div>
                } else {
                    <canvas
                        ref={canvas_ref}
                        role="img"
                        aria-label={format!("3D crosswalk scene: {}", props.graph.sides.iter().map(|s| s.title.clone()).collect::<Vec<_>>().join(" mapped to "))}
                        class="crosswalk-scene__canvas"
                    />
                }

                if let (Some(t), Some((x, y))) = (tooltip.as_ref(), *pointer_screen) {
                    <div class="crosswalk-scene__tooltip" style={format!("left:{}px;top:{}px", x + 14.0, y + 14.0)}>
                        <p class="crosswalk-scene__tooltip-title">{ t.title.clone() }</p>
                        if let Some(curie) = &t.curie {
                            <p class="crosswalk-scene__tooltip-curie">{ curie.clone() }</p>
                        }
                        if let Some(body) = &t.body {
                            <p class="crosswalk-scene__tooltip-body">{ body.clone() }</p>
                        }
                    </div>
                }

                if let Some(n) = selected_node {
                    <div class="crosswalk-scene__inspector">
                        <div class="crosswalk-scene__inspector-head">
                            <div class="crosswalk-scene__inspector-title">
                                <span class="crosswalk-scene__swatch" style={format!("background-color:{}", n.color)} aria-hidden="true" />
                                <h4>{ n.label.clone() }</h4>
                            </div>
                            <button type="button" class="crosswalk-scene__inspector-close" aria-label="Close" onclick={{
                                let selected_iri = selected_iri.clone();
                                Callback::from(move |_| selected_iri.set(None))
                            }}>{ "\u{d7}" }</button>
                        </div>
                        <p class="crosswalk-scene__inspector-curie">{ n.curie.clone() }</p>
                        if let Some(desc) = &n.description {
                            <p class="crosswalk-scene__inspector-desc">{ desc.clone() }</p>
                        }
                        if !selected_mappings.is_empty() {
                            <div class="crosswalk-scene__inspector-mappings">
                                <p class="crosswalk-scene__inspector-mappings-title">{ format!("Mapped to ({})", selected_mappings.len()) }</p>
                                <ul>
                                    { for selected_mappings.iter().map(|(e, partner)| {
                                        let partner_iri = partner.map(|p| p.iri.clone());
                                        let label = partner.map(|p| p.label.clone()).unwrap_or_else(|| if selected_node.map(|n| n.iri == e.source).unwrap_or(false) { e.target.clone() } else { e.source.clone() });
                                        html! {
                                            <li key={format!("{}-{}", e.source, e.target)}>
                                                <button type="button" class="crosswalk-scene__inspector-link" onclick={{
                                                    let selected_iri = selected_iri.clone();
                                                    Callback::from(move |_| selected_iri.set(partner_iri.clone()))
                                                }}>{ label }</button>
                                                <span class="crosswalk-scene__inspector-match">{ format!(" \u{2014} {}Match", e.matched.map(|m| m.label()).unwrap_or("")) }</span>
                                                if let Some(r) = &e.rationale {
                                                    <p class="crosswalk-scene__inspector-rationale">{ r.clone() }</p>
                                                }
                                            </li>
                                        }
                                    }) }
                                </ul>
                            </div>
                        }
                    </div>
                }
            </div>

            <details class="crosswalk-scene__table-details">
                <summary>{ format!("Mapping table ({})", props.graph.stats.mapped) }</summary>
                <div class="crosswalk-scene__table-wrap">
                    <table class="crosswalk-scene__table">
                        <thead>
                            <tr><th>{ "Term" }</th><th>{ "Match" }</th><th>{ "Term" }</th><th>{ "Rationale" }</th></tr>
                        </thead>
                        <tbody>
                            { for props.graph.edges.iter().filter(|e| e.kind == XwalkEdgeKind::Mapping).map(|e| html! {
                                <tr key={format!("{}-{}", e.source, e.target)}>
                                    <td>{ by_iri.get(e.source.as_str()).map(|n| n.label.clone()).unwrap_or_else(|| e.source.clone()) }</td>
                                    <td>{ format!("{}Match", e.matched.map(|m| m.label()).unwrap_or("")) }</td>
                                    <td>{ by_iri.get(e.target.as_str()).map(|n| n.label.clone()).unwrap_or_else(|| e.target.clone()) }</td>
                                    <td>{ e.rationale.clone().unwrap_or_default() }</td>
                                </tr>
                            }) }
                        </tbody>
                    </table>
                </div>
            </details>
        </div>
    }
}
