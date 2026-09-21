/* EdgeFabric Live Viewer — PhoneTwin 3D runtime (Three.js, no modules).
 * Global: window.EfPhoneTwin { init, refresh, setRes, setFold, setModel,
 *                              setEnv, pick, dispose, isReady }
 *
 * Models: 'iphone-18-pro' (6.3" bar, smaller Dynamic Island, triple camera),
 *         'iphone-duo'    (book foldable: 5.4" cover + 7.6" inner display),
 *         'iphone-generic' (fallback bar for older bezels).
 *
 * The live simulator screenshot <img> is used directly as the screen texture
 * (same-origin blob: URL, so WebGL upload is allowed). Rust calls refresh()
 * whenever a new frame lands. Taps/swipes are resolved via pick() which
 * raycasts the screen meshes and returns device pixels.
 */
(function () {
  'use strict';
  var twins = {};
  window.__ef_phone_twins = twins;

  function isReady() {
    return typeof window.THREE !== 'undefined';
  }

  // ── helpers ──────────────────────────────────────────────────────

  function roundedRectShape(w, h, r) {
    var s = new THREE.Shape();
    var x = -w / 2, y = -h / 2;
    s.moveTo(x + r, y);
    s.lineTo(x + w - r, y);
    s.quadraticCurveTo(x + w, y, x + w, y + r);
    s.lineTo(x + w, y + h - r);
    s.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
    s.lineTo(x + r, y + h);
    s.quadraticCurveTo(x, y + h, x, y + h - r);
    s.lineTo(x, y + r);
    s.quadraticCurveTo(x, y, x + r, y);
    return s;
  }

  function roundedSlab(w, h, d, r, mat) {
    var geo = new THREE.ExtrudeGeometry(roundedRectShape(w, h, r), {
      depth: d - r * 0.6,
      bevelEnabled: true,
      bevelThickness: r * 0.3,
      bevelSize: r * 0.3,
      bevelSegments: 3,
      curveSegments: 24
    });
    geo.center();
    var m = new THREE.Mesh(geo, mat);
    m.castShadow = true;
    m.receiveShadow = true;
    return m;
  }

  // ShapeGeometry emits UVs in raw shape units — normalize to 0..1 so the
  // live texture maps edge-to-edge instead of a clamped center patch.
  function normalizeUVs(geo, w, h) {
    var uv = geo.attributes.uv;
    var pos = geo.attributes.position;
    for (var i = 0; i < uv.count; i++) {
      uv.setXY(i, (pos.getX(i) + w / 2) / w, (pos.getY(i) + h / 2) / h);
    }
    uv.needsUpdate = true;
  }

  function flatScreen(w, h, r, tex, tint) {
    var geo = new THREE.ShapeGeometry(roundedRectShape(w, h, r), 24);
    normalizeUVs(geo, w, h);
    var mat = new THREE.MeshBasicMaterial({ color: 0xffffff, toneMapped: false });
    if (tex) mat.map = tex;
    else mat.color.set(tint || 0x05070c);
    var m = new THREE.Mesh(geo, mat);
    return m;
  }

  function halfTexture(img, leftHalf) {
    var t = new THREE.Texture(img);
    t.needsUpdate = true;
    t.minFilter = THREE.LinearFilter;
    t.generateMipmaps = false;
    if (t.encoding !== undefined && THREE.sRGBEncoding !== undefined) t.encoding = THREE.sRGBEncoding;
    t.repeat.set(0.5, 1);
    t.offset.set(leftHalf ? 0 : 0.5, 0);
    t.wrapS = THREE.ClampToEdgeWrapping;
    return t;
  }

  function screenTexture(img) {
    var t = new THREE.Texture(img);
    t.needsUpdate = true;
    t.minFilter = THREE.LinearFilter;
    t.generateMipmaps = false;
    if (t.encoding !== undefined && THREE.sRGBEncoding !== undefined) t.encoding = THREE.sRGBEncoding;
    return t;
  }

  function makeLabel(text, w, h, size) {
    var canvas = document.createElement('canvas');
    canvas.width = 512; canvas.height = 128;
    var ctx = canvas.getContext('2d');
    ctx.clearRect(0, 0, 512, 128);
    ctx.font = '600 ' + (size || 40) + 'px -apple-system, Helvetica, Arial, sans-serif';
    ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
    ctx.fillStyle = 'rgba(210,218,228,0.9)';
    ctx.fillText(String(text).slice(0, 30), 256, 64);
    var tex = new THREE.CanvasTexture(canvas);
    var sp = new THREE.Sprite(new THREE.SpriteMaterial({ map: tex, transparent: true, depthWrite: false }));
    sp.scale.set(w || 1.6, h || 0.4, 1);
    return sp;
  }

  function lens(r, h) {
    var g = new THREE.Group();
    var ring = new THREE.Mesh(
      new THREE.CylinderGeometry(r, r, h * 0.5, 28),
      new THREE.MeshStandardMaterial({ color: 0x8f97a3, roughness: 0.3, metalness: 0.9 })
    );
    ring.rotation.x = Math.PI / 2;
    g.add(ring);
    var glass = new THREE.Mesh(
      new THREE.CylinderGeometry(r * 0.72, r * 0.72, h * 0.55, 28),
      new THREE.MeshStandardMaterial({ color: 0x0a1626, roughness: 0.08, metalness: 0.6 })
    );
    glass.rotation.x = Math.PI / 2;
    glass.position.z = h * 0.06;
    g.add(glass);
    var glint = new THREE.Mesh(
      new THREE.CircleGeometry(r * 0.22, 16),
      new THREE.MeshBasicMaterial({ color: 0x9fc4e8, transparent: true, opacity: 0.7 })
    );
    glint.position.set(-r * 0.25, r * 0.25, h * 0.35);
    g.add(glint);
    return g;
  }

  // ── lifecycle ────────────────────────────────────────────────────

  function dispose(id) {
    var t = twins[id];
    if (!t) return;
    try { cancelAnimationFrame(t.raf); } catch (e) {}
    try {
      if (t.renderer) {
        t.renderer.dispose();
        if (t.renderer.domElement && t.renderer.domElement.parentNode) {
          t.renderer.domElement.parentNode.removeChild(t.renderer.domElement);
        }
      }
    } catch (e) {}
    try {
      if (t.el && t.el._efPhoneHandlers) {
        var h = t.el._efPhoneHandlers;
        t.el.removeEventListener('pointerdown', h.down);
        window.removeEventListener('pointermove', h.move);
        window.removeEventListener('pointerup', h.up);
        t.el.removeEventListener('wheel', h.wheel);
        t.el._efPhoneHandlers = null;
      }
      if (t.resizeHandler) window.removeEventListener('resize', t.resizeHandler);
    } catch (e) {}
    delete twins[id];
  }

  function baseScene(el, W, H) {
    var renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    renderer.setSize(W, H);
    renderer.shadowMap.enabled = true;
    renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    if (THREE.sRGBEncoding !== undefined) renderer.outputEncoding = THREE.sRGBEncoding;
    el.appendChild(renderer.domElement);

    var scene = new THREE.Scene();
    scene.fog = new THREE.Fog(0x050b14, 16, 34);
    var camera = new THREE.PerspectiveCamera(38, W / H, 0.1, 100);

    var hemi = new THREE.HemisphereLight(0xcfe4ff, 0x0a1420, 0.9);
    scene.add(hemi);
    var key = new THREE.DirectionalLight(0xfff1de, 1.5);
    key.position.set(5, 8, 6);
    key.castShadow = true;
    key.shadow.mapSize.set(1024, 1024);
    key.shadow.camera.left = -8; key.shadow.camera.right = 8;
    key.shadow.camera.top = 8; key.shadow.camera.bottom = -8;
    scene.add(key);
    var rim = new THREE.DirectionalLight(0x7dd3fc, 0.65);
    rim.position.set(-6, 4, -6);
    scene.add(rim);

    var ground = new THREE.Mesh(
      new THREE.PlaneGeometry(34, 34),
      new THREE.MeshStandardMaterial({ color: 0x0a1220, roughness: 0.9, metalness: 0.05 })
    );
    ground.rotation.x = -Math.PI / 2;
    ground.position.y = -4.4;
    ground.receiveShadow = true;
    scene.add(ground);
    var grid = new THREE.GridHelper(26, 26, 0x1e3a5f, 0x12233a);
    grid.position.y = -4.39;
    grid.material.transparent = true;
    grid.material.opacity = 0.4;
    scene.add(grid);

    var rig = new THREE.Group();
    scene.add(rig);
    return { renderer: renderer, scene: scene, camera: camera, rig: rig };
  }

  function orbitControls(el, orbit) {
    var dragging = false, px = 0, py = 0;
    function down(e) {
      dragging = true; px = e.clientX; py = e.clientY;
      try { el.setPointerCapture(e.pointerId); } catch (err) {}
    }
    function move(e) {
      if (!dragging) return;
      var dx = e.clientX - px, dy = e.clientY - py;
      px = e.clientX; py = e.clientY;
      orbit.yaw -= dx * 0.008;
      orbit.pitch = Math.max(0.12, Math.min(1.4, orbit.pitch + dy * 0.006));
    }
    function up() { dragging = false; }
    function wheel(e) {
      e.preventDefault();
      orbit.dist = Math.max(5, Math.min(20, orbit.dist + (e.deltaY > 0 ? 0.7 : -0.7)));
    }
    el._efPhoneHandlers = { down: down, move: move, up: up, wheel: wheel };
    el.addEventListener('pointerdown', down);
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    el.addEventListener('wheel', wheel, { passive: false });
    return {
      isDragging: function () { return dragging; }
    };
  }

  // ── iPhone 18 Pro ───────────────────────────────────────────────
  // 6.3" bar · 150 x 71.9 x 8.8mm · smaller Dynamic Island · triple 48MP.

  function build18Pro(group, img, res) {
    var W = 3.6, H = 7.5, D = 0.44;
    var titanium = new THREE.MeshStandardMaterial({ color: 0x9aa0a8, roughness: 0.32, metalness: 0.92 });
    var backGlass = new THREE.MeshStandardMaterial({ color: 0x4a2b33, roughness: 0.18, metalness: 0.35 });

    var body = roundedSlab(W, H, D, 0.55, titanium);
    group.add(body);
    // Back glass inlay (burgundy hero finish)
    var back = new THREE.Mesh(
      new THREE.ShapeGeometry(roundedRectShape(W - 0.1, H - 0.1, 0.5), 20),
      backGlass
    );
    back.position.z = -D / 2 - 0.001;
    back.rotation.y = Math.PI;
    group.add(back);

    // Live screen
    var tex = img ? screenTexture(img) : null;
    var scr = flatScreen(W - 0.28, H - 0.28, 0.42, tex, 0x05070c);
    scr.position.z = D / 2 + 0.012;
    group.add(scr);

    // Smaller Dynamic Island pill (~55% of 14-Pro width)
    var island = roundedSlab(0.62, 0.17, 0.03, 0.085, new THREE.MeshBasicMaterial({ color: 0x000000 }));
    island.position.set(0, H / 2 - 0.42, D / 2 + 0.02);
    group.add(island);
    var fcam = new THREE.Mesh(
      new THREE.CircleGeometry(0.045, 20),
      new THREE.MeshStandardMaterial({ color: 0x0a1a2e, roughness: 0.1, metalness: 0.5 })
    );
    fcam.position.set(0.16, H / 2 - 0.42, D / 2 + 0.04);
    group.add(fcam);

    // Camera plateau (rounded square, top-left of back)
    var plateau = roundedSlab(1.7, 1.7, 0.12, 0.3, new THREE.MeshStandardMaterial({ color: 0x3a2126, roughness: 0.25, metalness: 0.5 }));
    plateau.position.set(-W / 2 + 0.95, H / 2 - 0.95, -D / 2 - 0.06);
    group.add(plateau);
    var lensPos = [[-0.42, 0.42], [0.42, 0.42], [0, -0.42]];
    lensPos.forEach(function (p) {
      var l = lens(0.3, 0.14);
      l.position.set(-W / 2 + 0.95 + p[0], H / 2 - 0.95 + p[1], -D / 2 - 0.14);
      l.rotation.x = 0;
      // lenses face -z (out the back)
      l.rotation.y = Math.PI;
      group.add(l);
    });
    var lidar = new THREE.Mesh(
      new THREE.CircleGeometry(0.11, 20),
      new THREE.MeshStandardMaterial({ color: 0x111418, roughness: 0.2, metalness: 0.4 })
    );
    lidar.position.set(-W / 2 + 1.62, H / 2 - 1.62, -D / 2 - 0.125);
    lidar.rotation.y = Math.PI;
    group.add(lidar);
    var flash = new THREE.Mesh(
      new THREE.CircleGeometry(0.09, 20),
      new THREE.MeshStandardMaterial({ color: 0xf5edd6, roughness: 0.4, emissive: 0x554411, emissiveIntensity: 0.2 })
    );
    flash.position.set(-W / 2 + 1.62, H / 2 - 0.55, -D / 2 - 0.125);
    flash.rotation.y = Math.PI;
    group.add(flash);

    // Side buttons: Action + volume (left), power + Camera Control (right)
    var btnMat = new THREE.MeshStandardMaterial({ color: 0x7d838c, roughness: 0.35, metalness: 0.9 });
    function sideBtn(x, y, w, h) {
      var b = new THREE.Mesh(new THREE.BoxGeometry(w, h, 0.06), btnMat);
      b.position.set(x, y, 0);
      group.add(b);
    }
    sideBtn(-W / 2 - 0.02, 2.2, 0.05, 0.32);  // Action
    sideBtn(-W / 2 - 0.02, 1.55, 0.05, 0.5);  // Vol up
    sideBtn(-W / 2 - 0.02, 0.95, 0.05, 0.5);  // Vol down
    sideBtn(W / 2 + 0.02, 1.3, 0.05, 0.62);   // Power
    sideBtn(W / 2 + 0.02, -0.5, 0.05, 0.85);  // Camera Control

    var tag = makeLabel('iPhone 18 Pro · A20 Pro', 2.4, 0.32, 34);
    tag.position.set(0, -H / 2 - 0.55, 0.4);
    group.add(tag);

    return {
      screens: [{ mesh: scr, kind: 'full' }],
      textures: tex ? [tex] : [],
      halfH: H / 2, halfW: W / 2
    };
  }

  // ── iPhone Duo (foldable) ───────────────────────────────────────
  // Book fold · 5.4" cover · 7.6" inner · hinge · Touch ID · dual camera.

  function buildDuo(group, img, res) {
    var SW = 2.9, SH = 7.6, SD = 0.3; // per slab
    var ti = new THREE.MeshStandardMaterial({ color: 0xb9bec7, roughness: 0.3, metalness: 0.92 });
    var tiDark = new THREE.MeshStandardMaterial({ color: 0x6b7078, roughness: 0.4, metalness: 0.9 });

    var left = new THREE.Group();
    var leftSlab = roundedSlab(SW, SH, SD, 0.4, ti);
    left.add(leftSlab);
    left.position.x = -SW / 2;
    group.add(left);

    // Hinge barrel (segmented — suggests the 100+ component hinge)
    var hinge = new THREE.Group();
    for (var hi = 0; hi < 9; hi++) {
      var seg = new THREE.Mesh(
        new THREE.CylinderGeometry(0.16, 0.16, SH / 9 - 0.03, 18),
        hi % 2 ? ti : tiDark
      );
      seg.position.y = -SH / 2 + (hi + 0.5) * (SH / 9);
      seg.castShadow = true;
      hinge.add(seg);
    }
    group.add(hinge);

    // Right slab pivots around the hinge (x=0). Open = flat (rot 0),
    // closed = folded back over the left slab (rot -PI).
    var rightPivot = new THREE.Group();
    var rightSlab = roundedSlab(SW, SH, SD, 0.4, ti);
    rightSlab.position.x = SW / 2;
    rightPivot.add(rightSlab);
    group.add(rightPivot);

    // Inner displays (continuous live image split across halves)
    var texL = img ? halfTexture(img, true) : null;
    var texR = img ? halfTexture(img, false) : null;
    var innerL = flatScreen(SW - 0.24, SH - 0.24, 0.3, texL, 0x05070c);
    innerL.position.set(-SW / 2, 0, SD / 2 + 0.012);
    group.add(innerL);
    var innerR = flatScreen(SW - 0.24, SH - 0.24, 0.3, texR, 0x05070c);
    innerR.position.x = SW / 2;
    rightPivot.add(innerR);
    // Inner bezels read as one surface: hide the slab gap with a spine strip
    var spine = new THREE.Mesh(
      new THREE.BoxGeometry(0.1, SH - 0.2, 0.02),
      new THREE.MeshBasicMaterial({ color: 0x000000 })
    );
    spine.position.set(0, 0, SD / 2 + 0.014);
    group.add(spine);

    // Cover display on the outer face of the left slab (5.4")
    var texC = img ? screenTexture(img) : null;
    var cover = flatScreen(SW - 0.5, SH - 0.9, 0.34, texC, 0x05070c);
    cover.position.set(-SW / 2, 0, -SD / 2 - 0.012);
    cover.rotation.y = Math.PI;
    group.add(cover);
    // Cover punch-hole camera
    var hole = new THREE.Mesh(
      new THREE.CircleGeometry(0.05, 18),
      new THREE.MeshBasicMaterial({ color: 0x000000 })
    );
    hole.position.set(-SW / 2, SH / 2 - 0.75, -SD / 2 - 0.02);
    hole.rotation.y = Math.PI;
    group.add(hole);

    // Dual camera bar on the outer face of the right slab
    var camBar = roundedSlab(1.15, 0.62, 0.1, 0.2, tiDark);
    camBar.position.set(SW / 2, SH / 2 - 1.0, -SD / 2 - 0.05);
    rightPivot.add(camBar);
    var dl1 = lens(0.24, 0.12);
    dl1.position.set(SW / 2 - 0.28, SH / 2 - 1.0, -SD / 2 - 0.12);
    dl1.rotation.y = Math.PI;
    rightPivot.add(dl1);
    var dl2 = lens(0.24, 0.12);
    dl2.position.set(SW / 2 + 0.28, SH / 2 - 1.0, -SD / 2 - 0.12);
    dl2.rotation.y = Math.PI;
    rightPivot.add(dl2);

    // Touch ID side button (no Face ID on Duo)
    var touchId = new THREE.Mesh(
      new THREE.BoxGeometry(0.05, 0.7, 0.1),
      new THREE.MeshStandardMaterial({ color: 0xd7dce3, roughness: 0.3, metalness: 0.9 })
    );
    touchId.position.set(-SW - 0.02, 1.4, 0);
    group.add(touchId);

    var tag = makeLabel('iPhone Duo · 7.6" FlexOLED', 2.6, 0.32, 34);
    tag.position.set(0, -SH / 2 - 0.55, 0.6);
    group.add(tag);

    var textures = [];
    if (texL) textures.push(texL);
    if (texR) textures.push(texR);
    if (texC) textures.push(texC);

    return {
      screens: [
        { mesh: innerL, kind: 'half-left' },
        { mesh: innerR, kind: 'half-right' },
        { mesh: cover, kind: 'full' }
      ],
      textures: textures,
      rightPivot: rightPivot,
      halfH: SH / 2, halfW: SW
    };
  }

  // ── init ─────────────────────────────────────────────────────────

  function liveImg() {
    return document.getElementById('ef-live-screen');
  }

  function init(id, opts) {
    opts = opts || {};
    if (!isReady()) return { ok: false, reason: 'three-missing' };
    dispose(id);
    var el = document.getElementById(id);
    if (!el) return { ok: false, reason: 'no-container' };

    var model = opts.model || 'iphone-generic';
    if (model !== 'iphone-18-pro' && model !== 'iphone-duo') model = 'iphone-generic';

    var W = el.clientWidth || 560, H = el.clientHeight || 520;
    var base = baseScene(el, W, H);
    var renderer = base.renderer, scene = base.scene, camera = base.camera, rig = base.rig;

    var phone = new THREE.Group();
    rig.add(phone);

    var img = liveImg();
    var hasFrame = !!(img && img.currentSrc);
    var built;
    if (model === 'iphone-duo') built = buildDuo(phone, hasFrame ? img : null);
    else built = build18Pro(phone, hasFrame ? img : null);
    // Generic fallback shares the 18 Pro body with a standard island.
    if (model === 'iphone-generic') {
      var tag2 = makeLabel('3D preview', 1.8, 0.3, 32);
      tag2.position.set(0, -4.35, 0.4);
      phone.add(tag2);
    }

    var t = {
      el: el,
      renderer: renderer,
      scene: scene,
      camera: camera,
      rig: rig,
      phone: phone,
      built: built,
      model: model,
      resW: opts.resW || 1206,
      resH: opts.resH || 2622,
      fold: typeof opts.fold === 'number' ? opts.fold : 1,
      autoRotate: opts.autoRotate !== false,
      orbit: { yaw: -0.55, pitch: 0.5, dist: model === 'iphone-duo' ? 13.5 : 11 },
      raf: 0,
      raycaster: new THREE.Raycaster()
    };
    applyFold(t);
    orbitControls(el, t.orbit);
    twins[id] = t;

    var clock = new THREE.Clock();
    t.dragging = false;
    el.addEventListener('pointerdown', function () { t.dragging = true; });
    window.addEventListener('pointerup', function () { t.dragging = false; });

    function frame() {
      t.raf = requestAnimationFrame(frame);
      var dt = Math.min(0.05, clock.getDelta());
      var tm = clock.elapsedTime;
      // auto-rotate pauses while the user drags the model
      if (t.autoRotate && !t.dragging) t.orbit.yaw += dt * 0.16;
      var o = t.orbit;
      camera.position.set(
        Math.sin(o.yaw) * Math.cos(o.pitch) * o.dist,
        Math.sin(o.pitch) * o.dist * 0.9 + 0.4,
        Math.cos(o.yaw) * Math.cos(o.pitch) * o.dist
      );
      camera.lookAt(0, 0, 0);
      phone.position.y = Math.sin(tm * 1.1) * 0.05;
      renderer.render(scene, camera);
    }
    frame();

    function onResize() {
      var w = el.clientWidth || W, h = el.clientHeight || H;
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      renderer.setSize(w, h);
    }
    t.resizeHandler = onResize;
    window.addEventListener('resize', onResize);
    return { ok: true };
  }

  function applyFold(t) {
    if (t.model !== 'iphone-duo' || !t.built.rightPivot) return;
    var f = Math.max(0, Math.min(1, t.fold));
    // fold 1 = fully open (flat), 0 = closed
    t.built.rightPivot.rotation.y = -(1 - f) * Math.PI * 0.96;
  }

  function refresh(id) {
    var t = twins[id];
    if (!t) return false;
    var img = liveImg();
    if (!img || !img.currentSrc) return false;
    var rebuilt = false;
    // First frame after init with no image: rebuild screens with texture.
    if (t.built.textures.length === 0) {
      var model = t.model;
      // remove old phone, rebuild with the live image
      t.rig.remove(t.phone);
      var phone = new THREE.Group();
      t.rig.add(phone);
      t.phone = phone;
      if (model === 'iphone-duo') t.built = buildDuo(phone, img);
      else t.built = build18Pro(phone, img);
      applyFold(t);
      rebuilt = true;
    } else {
      t.built.textures.forEach(function (tx) { tx.image = img; tx.needsUpdate = true; });
    }
    return rebuilt ? 'rebuilt' : true;
  }

  function setRes(id, w, h) {
    var t = twins[id];
    if (!t) return false;
    t.resW = w; t.resH = h;
    return true;
  }

  function setFold(id, f) {
    var t = twins[id];
    if (!t) return false;
    t.fold = Math.max(0, Math.min(1, f));
    applyFold(t);
    return true;
  }

  function setEnv(id, env) {
    var t = twins[id];
    if (!t) return false;
    if (env.autoRotate != null) t.autoRotate = !!env.autoRotate;
    return true;
  }

  // Raycast a client point onto the live screens → device pixels JSON.
  function pick(id, clientX, clientY) {
    var t = twins[id];
    if (!t) return null;
    var rect = t.renderer.domElement.getBoundingClientRect();
    var nx = ((clientX - rect.left) / rect.width) * 2 - 1;
    var ny = -((clientY - rect.top) / rect.height) * 2 + 1;
    t.raycaster.setFromCamera({ x: nx, y: ny }, t.camera);
    var meshes = t.built.screens.map(function (s) { return s.mesh; });
    var hits = t.raycaster.intersectObjects(meshes, false);
    if (!hits.length || !hits[0].uv) return null;
    var u = hits[0].uv.x, v = hits[0].uv.y;
    var kind = t.built.screens[meshes.indexOf(hits[0].object)].kind;
    var px, py;
    // Inner halves share one live image: left shows [0, .5], right shows [.5, 1].
    // v is flipped (flipY): v=1 is the top of the screenshot.
    if (kind === 'half-left') { px = u * 0.5 * t.resW; py = (1 - v) * t.resH; }
    else if (kind === 'half-right') { px = (0.5 + u * 0.5) * t.resW; py = (1 - v) * t.resH; }
    else { px = u * t.resW; py = (1 - v) * t.resH; }
    // clamp
    px = Math.max(0, Math.min(t.resW - 1, Math.round(px)));
    py = Math.max(0, Math.min(t.resH - 1, Math.round(py)));
    return JSON.stringify({ x: px, y: py, mesh: kind });
  }

  window.EfPhoneTwin = {
    isReady: isReady,
    init: init,
    refresh: refresh,
    setRes: setRes,
    setFold: setFold,
    setEnv: setEnv,
    pick: pick,
    dispose: dispose,
    _twins: twins
  };
})();
