/* EdgeFabric Virtual IoT — BoardTwin 3D runtime (Three.js, no modules).
 * Global: window.EfBoardTwin { init, update, setEnv, dispose, isReady }
 * Boards supported: esp32-s3, esp32-c6, rpi-5 (+ generic fallback).
 * Custom orbit: drag = rotate, wheel = zoom. No OrbitControls dependency.
 */
(function () {
  'use strict';
  var twins = {};
  window.__ef_twins = twins;

  function isReady() {
    return typeof window.THREE !== 'undefined';
  }

  function hexColor(s, fallback) {
    try {
      if (typeof s === 'string' && /^#[0-9a-fA-F]{6}$/.test(s)) return parseInt(s.slice(1), 16);
    } catch (e) {}
    return fallback;
  }

  function boardDims(controller) {
    // Returns { w, d, h, kind } in scene units.
    if (controller === 'rpi-5') return { w: 5.6, d: 3.9, h: 0.16, kind: 'sbc' };
    if (controller === 'esp32-s3') return { w: 2.1, d: 3.4, h: 0.14, kind: 'devkit20' };
    if (controller === 'esp32-c6') return { w: 1.9, d: 2.9, h: 0.14, kind: 'devkit12' };
    return { w: 2.0, d: 3.2, h: 0.14, kind: 'devkit' };
  }

  function makeLabelSprite(text, opts) {
    opts = opts || {};
    var size = opts.size || 44;
    var canvas = document.createElement('canvas');
    canvas.width = 512; canvas.height = 128;
    var ctx = canvas.getContext('2d');
    ctx.clearRect(0, 0, 512, 128);
    ctx.font = '600 ' + size + 'px ui-monospace, Menlo, monospace';
    ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
    ctx.fillStyle = opts.color || 'rgba(226,232,240,0.92)';
    ctx.fillText(String(text).slice(0, 28), 256, 64);
    var tex = new THREE.CanvasTexture(canvas);
    tex.anisotropy = 4;
    var mat = new THREE.SpriteMaterial({ map: tex, transparent: true, depthWrite: false });
    var sp = new THREE.Sprite(mat);
    sp.scale.set(opts.w || 2.2, opts.h || 0.55, 1);
    return sp;
  }

  function sensorLevel(key, value) {
    // Normalise a sensor reading string to 0..1 for glow/height mapping.
    if (value == null) return 0.35;
    var s = String(value).trim();
    var nums = s.split(',').map(function (p) { var v = parseFloat(p); return isFinite(v) ? v : NaN; }).filter(function (v) { return !isNaN(v); });
    function clamp01(v) { return Math.max(0, Math.min(1, v)); }
    if (key === 'temperature') { var t = nums[0]; if (!isFinite(t)) return 0.3; return clamp01((t + 10) / 60); }
    if (key === 'humidity') { var h = nums[0]; if (!isFinite(h)) return 0.3; return clamp01(h / 100); }
    if (key === 'pressure') { var p = nums[0]; if (!isFinite(p)) return 0.3; return clamp01((p - 950) / 100); }
    if (key === 'ambient_light') { var l = nums[0]; if (!isFinite(l)) return 0.3; return clamp01(Math.log10(1 + l) / 5); }
    if (key === 'uv_index') { var u = nums[0]; if (!isFinite(u)) return 0.2; return clamp01(u / 11); }
    if (key === 'proximity') { var d = nums[0]; if (!isFinite(d)) return 0.3; return clamp01(1 - d / 150); }
    if (key === 'battery_voltage') { var v = nums[0]; if (!isFinite(v)) return 0.5; return clamp01((v - 3.0) / 1.2); }
    if (key === 'current') { var c = nums[0]; if (!isFinite(c)) return 0.3; return clamp01(c / 600); }
    if (key === 'wifi_rssi' || key === 'ble_scan') { var r = nums[0]; if (!isFinite(r)) return 0.3; return clamp01((r + 95) / 60); }
    if (key === 'sound_level') { var db = nums[0]; if (!isFinite(db)) return 0.3; return clamp01((db - 30) / 70); }
    if (key === 'air_quality_co2') { var co2 = nums[0]; if (!isFinite(co2)) return 0.3; return clamp01((co2 - 400) / 1800); }
    if (key === 'air_quality_tvoc') { var tv = nums[0]; if (!isFinite(tv)) return 0.3; return clamp01(tv / 600); }
    if (key === 'adc_raw') { var a = nums[0]; if (!isFinite(a)) return 0.3; return clamp01(a / 4095); }
    if (key === 'gpio_states' || key.indexOf('gpio_') === 0) { return (s === '1' || s.toLowerCase().indexOf('high') >= 0) ? 1 : 0.08; }
    if (key === 'accel_xyz' || key === 'gyro_xyz' || key === 'magnetometer') {
      var m = Math.sqrt(nums.reduce(function (acc, v) { return acc + v * v; }, 0));
      if (!isFinite(m) || m === 0) return 0.3;
      return clamp01(m / 25);
    }
    if (key === 'gps') return 0.55;
    return 0.4;
  }

  function dispose(id) {
    var t = twins[id];
    if (!t) return;
    try { cancelAnimationFrame(t.raf); } catch (e) {}
    try {
      if (t.renderer) { t.renderer.dispose(); if (t.renderer.domElement && t.renderer.domElement.parentNode) t.renderer.domElement.parentNode.removeChild(t.renderer.domElement); }
    } catch (e) {}
    try {
      if (t.el && t.el._efHandlers) {
        var h = t.el._efHandlers;
        t.el.removeEventListener('pointerdown', h.down);
        window.removeEventListener('pointermove', h.move);
        window.removeEventListener('pointerup', h.up);
        t.el.removeEventListener('wheel', h.wheel);
        t.el._efHandlers = null;
      }
      if (t.resizeHandler) window.removeEventListener('resize', t.resizeHandler);
    } catch (e) {}
    delete twins[id];
  }

  function init(id, opts) {
    opts = opts || {};
    if (!isReady()) return { ok: false, reason: 'three-missing' };
    dispose(id);
    var el = document.getElementById(id);
    if (!el) return { ok: false, reason: 'no-container' };

    var controller = opts.controller || 'esp32-s3';
    var dims = boardDims(controller);
    var boardColor = hexColor(opts.boardColor, 0x1f6f43);
    var W = el.clientWidth || 560, H = el.clientHeight || 420;

    var renderer;
    try {
      renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    } catch (e) { return { ok: false, reason: 'webgl-failed' }; }
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    renderer.setSize(W, H);
    renderer.shadowMap.enabled = true;
    renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    el.appendChild(renderer.domElement);

    var scene = new THREE.Scene();
    scene.fog = new THREE.Fog(0x050b14, 14, 30);

    var camera = new THREE.PerspectiveCamera(42, W / H, 0.1, 100);
    camera.position.set(5.2, 4.6, 6.4);

    // Lights
    var hemi = new THREE.HemisphereLight(0xbdd7ff, 0x0a1420, 0.85);
    scene.add(hemi);
    var sun = new THREE.DirectionalLight(0xfff2dd, 1.35);
    sun.position.set(5, 8, 3);
    sun.castShadow = true;
    sun.shadow.mapSize.set(1024, 1024);
    sun.shadow.camera.left = -8; sun.shadow.camera.right = 8;
    sun.shadow.camera.top = 8; sun.shadow.camera.bottom = -8;
    scene.add(sun);
    var rim = new THREE.DirectionalLight(0x67e8f9, 0.5);
    rim.position.set(-6, 3, -5);
    scene.add(rim);
    var nightLamp = new THREE.PointLight(0x38bdf8, 0.0, 20);
    nightLamp.position.set(0, 4.5, 0);
    scene.add(nightLamp);

    // Ground + grid (lab bench)
    var ground = new THREE.Mesh(
      new THREE.PlaneGeometry(30, 30),
      new THREE.MeshStandardMaterial({ color: 0x0a1220, roughness: 0.95, metalness: 0.0 })
    );
    ground.rotation.x = -Math.PI / 2;
    ground.position.y = -1.2;
    ground.receiveShadow = true;
    scene.add(ground);
    var grid = new THREE.GridHelper(24, 24, 0x1e3a5f, 0x12233a);
    grid.position.y = -1.19;
    grid.material.transparent = true;
    grid.material.opacity = 0.5;
    scene.add(grid);

    var rig = new THREE.Group(); // yaw/pitch rig (orbit)
    var tilt = new THREE.Group(); // motion tilt (accel/gyro)
    var boardGroup = new THREE.Group();
    tilt.add(boardGroup);
    rig.add(tilt);
    scene.add(rig);

    // PCB
    var pcbMat = new THREE.MeshStandardMaterial({ color: boardColor, roughness: 0.55, metalness: 0.25 });
    var pcb = new THREE.Mesh(new THREE.BoxGeometry(dims.w, dims.h, dims.d), pcbMat);
    pcb.castShadow = true; pcb.receiveShadow = true;
    boardGroup.add(pcb);
    // Soldermask gloss slab
    var gloss = new THREE.Mesh(
      new THREE.BoxGeometry(dims.w * 0.985, 0.012, dims.d * 0.985),
      new THREE.MeshStandardMaterial({ color: 0xffffff, roughness: 0.25, metalness: 0.1, transparent: true, opacity: 0.08 })
    );
    gloss.position.y = dims.h / 2 + 0.006;
    boardGroup.add(gloss);

    // Mounting holes
    var holeGeo = new THREE.CylinderGeometry(0.09, 0.09, dims.h + 0.04, 20);
    var holeMat = new THREE.MeshStandardMaterial({ color: 0xc7cfd8, roughness: 0.3, metalness: 0.9 });
    var holeDark = new THREE.MeshStandardMaterial({ color: 0x04070c, roughness: 0.9 });
    var hx = dims.w / 2 - 0.22, hz = dims.d / 2 - 0.22;
    [[-hx, -hz], [hx, -hz], [-hx, hz], [hx, hz]].forEach(function (p) {
      var ring = new THREE.Mesh(holeGeo, holeMat);
      ring.position.set(p[0], 0, p[1]);
      ring.castShadow = true;
      boardGroup.add(ring);
      var dark = new THREE.Mesh(new THREE.CylinderGeometry(0.045, 0.045, dims.h + 0.06, 14), holeDark);
      dark.position.set(p[0], 0, p[1]);
      boardGroup.add(dark);
    });

    // Main SoC
    var socW = controller === 'rpi-5' ? 0.95 : 0.72;
    var socD = controller === 'rpi-5' ? 0.95 : 0.72;
    var soc = new THREE.Mesh(
      new THREE.BoxGeometry(socW, 0.12, socD),
      new THREE.MeshStandardMaterial({ color: 0x141922, roughness: 0.4, metalness: 0.4 })
    );
    soc.position.set(controller === 'rpi-5' ? -0.9 : 0, dims.h / 2 + 0.06, controller === 'rpi-5' ? -0.5 : 0.35);
    soc.castShadow = true;
    boardGroup.add(soc);
    var socLabel = makeLabelSprite(opts.chipLabel || 'SoC', { size: 40, w: socW * 1.15, h: 0.3 });
    socLabel.position.set(soc.position.x, soc.position.y + 0.16, soc.position.z);
    boardGroup.add(socLabel);
    // SoC pins (teeth)
    var pinMat = new THREE.MeshStandardMaterial({ color: 0xcdd5e0, roughness: 0.35, metalness: 0.85 });
    var teeth = 8;
    for (var ti = 0; ti < teeth; ti++) {
      var tz = soc.position.z - socD / 2 + (ti + 0.5) * (socD / teeth);
      [-1, 1].forEach(function (s) {
        var tooth = new THREE.Mesh(new THREE.BoxGeometry(0.09, 0.03, socD / teeth * 0.55), pinMat);
        tooth.position.set(soc.position.x + s * (socW / 2 + 0.045), dims.h / 2 + 0.02, tz);
        boardGroup.add(tooth);
      });
    }

    // GPIO rails (devkits) or 40-pin header (rpi-5)
    var goldMat = new THREE.MeshStandardMaterial({ color: 0xd4a83a, roughness: 0.3, metalness: 0.9 });
    var pinCount = opts.pinCount || (dims.kind === 'sbc' ? 40 : 20);
    if (dims.kind === 'sbc') {
      var cols = 20, rows = 2;
      for (var r = 0; r < rows; r++) {
        for (var c = 0; c < cols; c++) {
          var px = -dims.w / 2 + 0.35 + c * ((dims.w - 0.7) / (cols - 1));
          var pz = -dims.d / 2 + 0.3 + r * 0.16;
          var hdr = new THREE.Mesh(new THREE.CylinderGeometry(0.035, 0.035, 0.22, 10), goldMat);
          hdr.position.set(px, dims.h / 2 + 0.11, pz);
          hdr.castShadow = true;
          boardGroup.add(hdr);
        }
      }
      // USB-C + HDMI + RJ45 blocks (stylised)
      function block(w, h, d, x, z, color, label) {
        var m = new THREE.Mesh(new THREE.BoxGeometry(w, h, d),
          new THREE.MeshStandardMaterial({ color: color, roughness: 0.35, metalness: 0.7 }));
        m.position.set(x, dims.h / 2 + h / 2, z);
        m.castShadow = true;
        boardGroup.add(m);
        if (label) {
          var sp = makeLabelSprite(label, { size: 34, w: w * 1.1, h: 0.22 });
          sp.position.set(x, dims.h / 2 + h + 0.14, z);
          boardGroup.add(sp);
        }
      }
      block(0.7, 0.34, 0.5, dims.w / 2 - 0.55, 0.9, 0x9aa4ae, 'USB-C');
      block(0.6, 0.3, 0.4, dims.w / 2 - 0.5, -0.2, 0x4a8bc4, 'USB3');
      block(0.7, 0.42, 0.55, dims.w / 2 - 0.55, -1.25, 0x8f969d, 'GbE');
      block(0.5, 0.18, 0.6, -dims.w / 2 + 0.5, 1.1, 0xb0b8be, 'HDMI');
      // PCIe FFC
      block(0.8, 0.06, 0.25, -1.2, 1.55, 0x3b4350, 'PCIe');
    } else {
      var perSide = Math.max(8, Math.min(20, Math.round(pinCount / 2)));
      for (var qi = 0; qi < perSide; qi++) {
        var py = dims.h / 2 + 0.03;
        var qz = -dims.d / 2 + 0.35 + qi * ((dims.d - 0.7) / Math.max(1, perSide - 1));
        [-1, 1].forEach(function (s) {
          var pad = new THREE.Mesh(new THREE.BoxGeometry(0.16, 0.03, 0.09), goldMat);
          pad.position.set(s * (dims.w / 2 - 0.12), py, qz);
          boardGroup.add(pad);
          var hole = new THREE.Mesh(new THREE.CylinderGeometry(0.028, 0.028, 0.05, 10),
            new THREE.MeshStandardMaterial({ color: 0x04070c }));
          hole.position.set(s * (dims.w / 2 - 0.12), py, qz);
          boardGroup.add(hole);
        });
      }
      // USB-C shell
      var usb = new THREE.Mesh(new THREE.BoxGeometry(0.55, 0.16, 0.28),
        new THREE.MeshStandardMaterial({ color: 0x9aa4ae, roughness: 0.3, metalness: 0.85 }));
      usb.position.set(0, dims.h / 2 + 0.08, dims.d / 2 + 0.1);
      usb.castShadow = true;
      boardGroup.add(usb);
      var mouth = new THREE.Mesh(new THREE.BoxGeometry(0.45, 0.07, 0.05),
        new THREE.MeshStandardMaterial({ color: 0x04070c, roughness: 0.9 }));
      mouth.position.set(0, dims.h / 2 + 0.08, dims.d / 2 + 0.24);
      boardGroup.add(mouth);
      // Antenna block (wireless)
      var ant = new THREE.Mesh(new THREE.BoxGeometry(0.6, 0.1, 0.5),
        new THREE.MeshStandardMaterial({ color: 0xc9cfd6, roughness: 0.5, metalness: 0.3 }));
      ant.position.set(0, dims.h / 2 + 0.05, -dims.d / 2 + 0.35);
      ant.castShadow = true;
      boardGroup.add(ant);
    }

    // LEDs (PWR green + ACT amber)
    function led(x, z, colorHex, label) {
      var g = new THREE.Group();
      var dome = new THREE.Mesh(new THREE.SphereGeometry(0.07, 18, 14),
        new THREE.MeshStandardMaterial({ color: colorHex, emissive: colorHex, emissiveIntensity: 0.9, roughness: 0.25 }));
      dome.position.y = 0.07;
      dome.castShadow = true;
      g.add(dome);
      var halo = new THREE.Mesh(new THREE.SphereGeometry(0.13, 14, 10),
        new THREE.MeshBasicMaterial({ color: colorHex, transparent: true, opacity: 0.22, depthWrite: false }));
      halo.position.y = 0.07;
      g.add(halo);
      var sp = makeLabelSprite(label, { size: 32, w: 0.5, h: 0.16 });
      sp.position.set(0, 0.3, 0);
      g.add(sp);
      g.position.set(x, dims.h / 2, z);
      g.userData.halo = halo;
      g.userData.dome = dome;
      boardGroup.add(g);
      return g;
    }
    var pwrLed = led(dims.w / 2 - 0.45, dims.d / 2 - 0.45, 0x22c55e, 'PWR');
    var actLed = led(dims.w / 2 - 0.45, dims.d / 2 - 0.85, 0xf59e0b, 'ACT');
    actLed.userData.dome.material.emissiveIntensity = 0.15;
    actLed.userData.halo.material.opacity = 0.05;

    // Sensor nodes ring
    var nodeGroup = new THREE.Group();
    boardGroup.add(nodeGroup);
    var wireGroup = new THREE.Group();
    boardGroup.add(wireGroup);
    var nodeMeshes = {};
    var wireMeshes = {};

    function busColor(bus) {
      var b = String(bus || '').toUpperCase();
      if (b.indexOf('WIFI') >= 0) return 0x3b82f6;
      if (b.indexOf('BLE') >= 0) return 0x8b5cf6;
      if (b.indexOf('I2C') >= 0) return 0x38bdf8;
      if (b.indexOf('SPI') >= 0) return 0xf59e0b;
      if (b.indexOf('UART') >= 0 || b.indexOf('SERIAL') >= 0) return 0x22c55e;
      if (b.indexOf('ADC') >= 0) return 0xef4444;
      if (b.indexOf('GPIO') >= 0 || b.indexOf('DIGITAL') >= 0) return 0xfacc15;
      if (b.indexOf('I2S') >= 0 || b.indexOf('USB') >= 0) return 0xfb7185;
      return 0x94a3b8;
    }

    function layoutNodes(sensors) {
      // Clear
      Object.keys(nodeMeshes).forEach(function (k) { nodeGroup.remove(nodeMeshes[k].g); });
      Object.keys(wireMeshes).forEach(function (k) { wireGroup.remove(wireMeshes[k]); });
      nodeMeshes = {}; wireMeshes = {};
      var n = sensors.length;
      if (!n) return;
      var R = Math.max(dims.w, dims.d) * 0.95 + 1.1;
      sensors.forEach(function (s, i) {
        var ang = (i / n) * Math.PI * 2 - Math.PI / 2;
        var nx = Math.cos(ang) * R, nz = Math.sin(ang) * R * 0.82;
        var color = s.color ? hexColor(s.color, busColor(s.bus)) : busColor(s.bus);
        var g = new THREE.Group();
        var box = new THREE.Mesh(new THREE.BoxGeometry(0.55, 0.22, 0.4),
          new THREE.MeshStandardMaterial({ color: 0x101a28, roughness: 0.5, metalness: 0.3 }));
        box.position.y = 0.35;
        box.castShadow = true;
        g.add(box);
        var glowMat = new THREE.MeshStandardMaterial({ color: color, emissive: color, emissiveIntensity: 0.55, roughness: 0.3 });
        var glow = new THREE.Mesh(new THREE.SphereGeometry(0.11, 16, 12), glowMat);
        glow.position.y = 0.62;
        g.add(glow);
        var halo = new THREE.Mesh(new THREE.SphereGeometry(0.2, 12, 8),
          new THREE.MeshBasicMaterial({ color: color, transparent: true, opacity: 0.18, depthWrite: false }));
        halo.position.y = 0.62;
        g.add(halo);
        var sp = makeLabelSprite(s.label || s.key, { size: 30, w: 1.15, h: 0.28 });
        sp.position.set(0, 0.95, 0);
        g.add(sp);
        g.position.set(nx, 0, nz);
        nodeGroup.add(g);
        nodeMeshes[s.key] = { g: g, glow: glowMat, halo: halo.material, baseY: 0, pulse: 0 };
        // Wire: curved line from node to board edge
        var start = new THREE.Vector3(nx, 0.35, nz);
        var endDir = new THREE.Vector3(-nx, 0, -nz).normalize();
        var end = new THREE.Vector3(endDir.x * (dims.w / 2 * 0.9), dims.h / 2 + 0.02, endDir.z * (dims.d / 2 * 0.9));
        var mid = start.clone().lerp(end, 0.5); mid.y = 0.9;
        var curve = new THREE.QuadraticBezierCurve3(start, mid, end);
        var geo = new THREE.BufferGeometry().setFromPoints(curve.getPoints(24));
        var line = new THREE.Line(geo, new THREE.LineBasicMaterial({ color: color, transparent: true, opacity: 0.75 }));
        wireGroup.add(line);
        wireMeshes[s.key] = line.material;
      });
    }

    var sensors0 = opts.sensors || [];
    layoutNodes(sensors0);
    var values0 = opts.values || {};
    applyValues(values0, null);

    function applyValues(values, pulseKey) {
      Object.keys(nodeMeshes).forEach(function (key) {
        var n = nodeMeshes[key];
        var lvl = sensorLevel(key, values[key]);
        n.glow.emissiveIntensity = 0.25 + lvl * 1.1 + (n.pulse || 0) * 1.6;
        n.halo.opacity = 0.08 + lvl * 0.22 + (n.pulse || 0) * 0.4;
        n.g.position.y = lvl * 0.35;
        if (pulseKey && key === pulseKey) n.pulse = 1.0;
      });
      // ACT LED flickers on any update
      actLed.userData.dome.material.emissiveIntensity = 1.1;
      actLed.userData.halo.material.opacity = 0.3;
      setTimeout(function () {
        try {
          actLed.userData.dome.material.emissiveIntensity = 0.35;
          actLed.userData.halo.material.opacity = 0.08;
        } catch (e) {}
      }, 220);
      // Motion tilt from accel/gyro
      var t = twins[id];
      if (t && t.env.tiltFromMotion) {
        var acc = String(values['accel_xyz'] || '').split(',').map(parseFloat);
        if (acc.length === 3 && acc.every(isFinite)) {
          var tx = Math.max(-0.45, Math.min(0.45, (acc[0] / 9.8) * 0.4));
          var tz = Math.max(-0.45, Math.min(0.45, (acc[1] / 9.8) * 0.4));
          t.targetTiltX = tz; t.targetTiltZ = -tx;
        }
      }
    }

    // Env state
    var env = {
      dayNight: typeof opts.dayNight === 'number' ? opts.dayNight : 0.75,
      autoRotate: opts.autoRotate !== false,
      tiltFromMotion: opts.tiltFromMotion !== false
    };

    function applyEnv() {
      var day = Math.max(0, Math.min(1, env.dayNight)); // 0=night,1=noon
      sun.intensity = 0.25 + day * 1.3;
      hemi.intensity = 0.25 + day * 0.7;
      sun.color.setHSL(0.1, 0.35, 0.55 + day * 0.25);
      nightLamp.intensity = (1 - day) * 1.6;
      scene.background = null;
      renderer.setClearColor(new THREE.Color().setHSL(0.6, 0.5, 0.03 + day * 0.05), 1);
      grid.material.opacity = 0.25 + day * 0.3;
    }
    applyEnv();

    // Orbit state (custom)
    var orbit = { yaw: 0.7, pitch: 0.62, dist: 8.6 };
    var targetTilt = { x: 0, z: 0 };
    var dragging = false, px = 0, py = 0, moved = false;

    function onDown(e) {
      dragging = true; moved = false;
      px = e.clientX; py = e.clientY;
      try { el.setPointerCapture(e.pointerId); } catch (err) {}
    }
    function onMove(e) {
      if (!dragging) return;
      var dx = e.clientX - px, dy = e.clientY - py;
      if (Math.abs(dx) + Math.abs(dy) > 2) moved = true;
      px = e.clientX; py = e.clientY;
      orbit.yaw -= dx * 0.008;
      orbit.pitch = Math.max(0.15, Math.min(1.35, orbit.pitch + dy * 0.006));
    }
    function onUp() { dragging = false; }
    function onWheel(e) {
      e.preventDefault();
      orbit.dist = Math.max(4.5, Math.min(16, orbit.dist + (e.deltaY > 0 ? 0.6 : -0.6)));
    }
    el._efHandlers = { down: onDown, move: onMove, up: onUp, wheel: onWheel };
    el.addEventListener('pointerdown', onDown);
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    el.addEventListener('wheel', onWheel, { passive: false });

    // Click (no-drag) on sensor node -> tooltip event
    el.addEventListener('click', function () {
      if (moved) return;
      // Reserved: selection handled Leptos-side via overlay list.
    });

    var clock = new THREE.Clock();
    var t0 = { targetTiltX: 0, targetTiltZ: 0, env: env, renderer: renderer, raf: 0, el: el };
    // Capture closure hooks so update()/setEnv() can drive this scene.
    t0.applier = applyValues;
    t0.applyEnvHook = applyEnv;
    twins[id] = t0;

    function frame() {
      t0.raf = requestAnimationFrame(frame);
      var dt = Math.min(0.05, clock.getDelta());
      var t = clock.elapsedTime;
      if (env.autoRotate && !dragging) orbit.yaw += dt * 0.18;
      var cx = Math.sin(orbit.yaw) * Math.cos(orbit.pitch) * orbit.dist;
      var cz = Math.cos(orbit.yaw) * Math.cos(orbit.pitch) * orbit.dist;
      var cy = Math.sin(orbit.pitch) * orbit.dist;
      camera.position.set(cx, cy, cz);
      camera.lookAt(0, 0.1, 0);
      // Ease tilt
      tilt.rotation.x += ((t0.targetTiltX || 0) - tilt.rotation.x) * Math.min(1, dt * 3);
      tilt.rotation.z += ((t0.targetTiltZ || 0) - tilt.rotation.z) * Math.min(1, dt * 3);
      // Idle bob + LED breathing + pulse decay
      boardGroup.position.y = Math.sin(t * 1.2) * 0.02;
      var br = 0.7 + Math.sin(t * 2.4) * 0.25;
      pwrLed.userData.halo.material.opacity = 0.15 + br * 0.12;
      Object.keys(nodeMeshes).forEach(function (k) {
        var n = nodeMeshes[k];
        if (n.pulse > 0) n.pulse = Math.max(0, n.pulse - dt * 1.6);
        n.g.rotation.y += dt * 0.25;
      });
      renderer.render(scene, camera);
    }
    frame();

    function onResize() {
      var w = el.clientWidth || W, h = el.clientHeight || H;
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      renderer.setSize(w, h);
    }
    t0.resizeHandler = onResize;
    window.addEventListener('resize', onResize);

    return { ok: true };
  }

  function update(id, data) {
    var t = twins[id];
    if (!t || !t.applier) return false;
    try {
      var el = document.getElementById(id);
      if (!el || !el.firstChild) return false;
    } catch (e) { return false; }
    // Caller re-inits when the sensor set changes; here we apply live
    // values plus an optional event pulse key.
    var sceneVals = (data && data.values) || {};
    var pulse = (data && data.pulse) || null;
    try { t.applier(sceneVals, pulse); return true; } catch (e) { return false; }
  }

  function relayout(id, sensors) {
    // Sensor set changed (enable/disable) — rebuild node ring in place.
    // Full re-init is cheaper to reason about; Rust side calls init again.
    return false;
  }

  window.EfBoardTwin = {
    isReady: isReady,
    init: init,
    update: update,
    relayout: relayout,
    setEnv: function (id, env) {
      var t = twins[id];
      if (!t || !t.env) return false;
      if (env.dayNight != null) t.env.dayNight = env.dayNight;
      if (env.autoRotate != null) t.env.autoRotate = env.autoRotate;
      if (env.tiltFromMotion != null) t.env.tiltFromMotion = env.tiltFromMotion;
      if (t.applyEnvHook) { try { t.applyEnvHook(); } catch (e) {} }
      return true;
    },
    dispose: dispose,
    _twins: twins
  };
})();
