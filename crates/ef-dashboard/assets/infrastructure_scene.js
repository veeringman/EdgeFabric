/* EdgeFabric infrastructure digital-twin renderer.
 * Renderer-neutral scene JSON in; no inventory or control-plane writes out.
 * Requires the dashboard's existing THREE global.
 */
(function () {
  const instances = new Map();
  const COLORS = {
    rack: 0x64748b, host: 0x2563eb, gpu: 0x8b5cf6, accelerator: 0x8b5cf6,
    pcie_switch: 0x0ea5e9, network_switch: 0x06b6d4, nic: 0x14b8a6,
    dpu: 0x10b981, storage_array: 0xf59e0b, nvme_ssd: 0xfbbf24,
    power_distribution_unit: 0xef4444, ups: 0xdc2626, cooling_zone: 0x38bdf8
  };
  const OVERLAY_COLORS = { pcie: 0x38bdf8, gpu: 0xa78bfa, network: 0x2dd4bf, storage: 0xfbbf24, power: 0xf87171 };

  function kindColor(kind) {
    const key = String(kind || "").toLowerCase();
    const match = Object.keys(COLORS).find(k => key.includes(k));
    return match ? COLORS[match] : 0x64748b;
  }
  function vec3(value, fallback) {
    return Array.isArray(value) && value.length >= 3
      ? new THREE.Vector3(Number(value[0]) || 0, Number(value[1]) || 0, Number(value[2]) || 0)
      : fallback.clone();
  }
  function makeLine(a, b, color, opacity) {
    const geometry = new THREE.BufferGeometry().setFromPoints([a, b]);
    const material = new THREE.LineBasicMaterial({ color, transparent: true, opacity });
    return new THREE.Line(geometry, material);
  }
  function init(containerId, document) {
    dispose(containerId);
    const container = document && document.getElementById ? document : window.document.getElementById(containerId);
    if (!container || !window.THREE || !document || !Array.isArray(document.nodes)) return;
    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x0b1220);
    scene.fog = new THREE.Fog(0x0b1220, 12, 55);
    const camera = new THREE.PerspectiveCamera(42, 1, 0.01, 250);
    camera.position.set(7, 6, 9);
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    renderer.shadowMap.enabled = true;
    renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    container.innerHTML = "";
    container.appendChild(renderer.domElement);

    scene.add(new THREE.HemisphereLight(0xcbd5e1, 0x172033, 1.8));
    const key = new THREE.DirectionalLight(0xffffff, 2.2);
    key.position.set(8, 12, 7); key.castShadow = true; scene.add(key);
    const fill = new THREE.DirectionalLight(0x60a5fa, 0.8);
    fill.position.set(-8, 4, -6); scene.add(fill);

    const grid = new THREE.GridHelper(24, 48, 0x334155, 0x1e293b);
    grid.position.y = -0.02; scene.add(grid);
    const root = new THREE.Group(); scene.add(root);
    const nodeById = new Map();
    const positions = new Map();
    const pending = new Set();

    // Resolve parent-relative rack/chassis transforms without assuming every
    // resource has a pose. Unplaced resources receive a deterministic layout.
    const sourceNodes = document.nodes;
    const nodesById = new Map(sourceNodes.map((n, i) => [n.id, { node: n, index: i }]));
    function positionFor(id, depth) {
      if (positions.has(id)) return positions.get(id).clone();
      if (depth > 8 || pending.has(id)) return new THREE.Vector3();
      pending.add(id);
      const entry = nodesById.get(id);
      const pose = entry && entry.node.transform;
      let p;
      if (pose && Array.isArray(pose.translation)) {
        p = vec3(pose.translation, new THREE.Vector3()).multiplyScalar(0.001);
        const frame = pose.coordinate_frame;
        if (frame && frame !== "site-mm" && nodesById.has(frame)) p.add(positionFor(frame, depth + 1));
      } else {
        const i = entry ? entry.index : 0;
        const columns = Math.max(1, Math.ceil(Math.sqrt(sourceNodes.length)));
        p = new THREE.Vector3((i % columns) * 1.2 - columns * 0.6, 0.35, Math.floor(i / columns) * 1.2 - columns * 0.35);
      }
      pending.delete(id);
      positions.set(id, p.clone());
      return p;
    }

    sourceNodes.forEach((node, index) => {
      const bounds = node.bounds_mm || {};
      const hasBounds = Number(bounds.width_mm) > 0 && Number(bounds.height_mm) > 0 && Number(bounds.depth_mm) > 0;
      const size = hasBounds
        ? new THREE.Vector3(bounds.width_mm, bounds.height_mm, bounds.depth_mm).multiplyScalar(0.001)
        : new THREE.Vector3(0.42, 0.24, 0.34);
      size.set(Math.max(size.x, 0.08), Math.max(size.y, 0.06), Math.max(size.z, 0.08));
      const kind = String(node.kind || "resource");
      const material = new THREE.MeshStandardMaterial({
        color: kindColor(kind), roughness: 0.48, metalness: 0.28,
        transparent: true, opacity: 0.94
      });
      const mesh = new THREE.Mesh(new THREE.BoxGeometry(size.x, size.y, size.z), material);
      mesh.position.copy(positionFor(node.id, 0));
      if (node.transform && Array.isArray(node.transform.rotation_xyzw)) {
        const q = node.transform.rotation_xyzw;
        mesh.quaternion.set(Number(q[0]) || 0, Number(q[1]) || 0, Number(q[2]) || 0, Number(q[3]) || 1);
      }
      mesh.castShadow = true; mesh.receiveShadow = true;
      mesh.userData.resourceId = node.id;
      root.add(mesh); nodeById.set(node.id, mesh);

      const edges = new THREE.EdgesGeometry(mesh.geometry);
      const outline = new THREE.LineSegments(edges, new THREE.LineBasicMaterial({ color: 0xcbd5e1, transparent: true, opacity: 0.28 }));
      outline.position.copy(mesh.position); outline.quaternion.copy(mesh.quaternion); root.add(outline);
    });

    // General graph relationships.
    (document.edges || []).forEach(edge => {
      const a = positions.get(edge.source), b = positions.get(edge.target);
      if (a && b) root.add(makeLine(a, b, 0x64748b, 0.24));
    });

    const overlayGroups = {};
    function addOverlay(name, links, sourceKey, targetKey) {
      const group = new THREE.Group();
      (links || []).forEach(link => {
        const a = positions.get(link[sourceKey]), b = positions.get(link[targetKey]);
        if (a && b) group.add(makeLine(a, b, OVERLAY_COLORS[name], 0.95));
      });
      root.add(group); overlayGroups[name] = group;
    }
    const overlays = document.topology_overlays || {};
    addOverlay("pcie", overlays.pcie && overlays.pcie.links, "upstream_identity", "downstream_identity");
    addOverlay("gpu", overlays.gpu_peer_fabric && overlays.gpu_peer_fabric.links, "source_identity", "target_identity");
    addOverlay("network", overlays.network && overlays.network.links, "source_identity", "target_identity");
    addOverlay("storage", overlays.storage && overlays.storage.links, "source_identity", "target_identity");
    if (overlays.cxl && overlays.cxl.links) {
      const group = new THREE.Group();
      overlays.cxl.links.forEach(link => {
        const a = positions.get(link.host_identity), b = positions.get(link.memory_device_identity);
        if (a && b) group.add(makeLine(a, b, OVERLAY_COLORS.storage, 0.95));
      });
      root.add(group); overlayGroups.storage.add(group);
    }
    // Power paths are represented by graph relationships; isolate powered_by
    // links into their own overlay group when those relationships are present.
    const powerGroup = new THREE.Group();
    (document.edges || []).filter(e => ["powered_by", "fed_by", "cooled_by"].includes(e.kind))
      .forEach(edge => {
        const a = positions.get(edge.source), b = positions.get(edge.target);
        if (a && b) powerGroup.add(makeLine(a, b, OVERLAY_COLORS.power, 0.95));
      });
    root.add(powerGroup); overlayGroups.power = powerGroup;

    const box = new THREE.Box3().setFromObject(root);
    const center = box.isEmpty() ? new THREE.Vector3() : box.getCenter(new THREE.Vector3());
    root.position.sub(center);
    const size = box.isEmpty() ? new THREE.Vector3(4, 3, 4) : box.getSize(new THREE.Vector3());
    const maxDim = Math.max(size.x, size.y, size.z, 2);
    camera.position.set(maxDim * 1.15, maxDim * 0.9, maxDim * 1.35);
    camera.lookAt(0, 0, 0);

    let dragging = false, panning = false, lastX = 0, lastY = 0;
    let theta = Math.atan2(camera.position.x, camera.position.z);
    let phi = Math.acos(camera.position.y / camera.position.length());
    let radius = camera.position.length();
    function updateCamera() {
      phi = Math.max(0.12, Math.min(Math.PI - 0.12, phi));
      radius = Math.max(1.2, Math.min(80, radius));
      camera.position.set(
        radius * Math.sin(phi) * Math.sin(theta),
        radius * Math.cos(phi),
        radius * Math.sin(phi) * Math.cos(theta)
      );
      camera.lookAt(target);
    }
    const target = new THREE.Vector3();
    function onDown(e) {
      dragging = true; panning = e.button === 2;
      lastX = e.clientX; lastY = e.clientY;
      renderer.domElement.setPointerCapture && renderer.domElement.setPointerCapture(e.pointerId);
    }
    function onMove(e) {
      if (!dragging) return;
      const dx = e.clientX - lastX, dy = e.clientY - lastY;
      lastX = e.clientX; lastY = e.clientY;
      if (panning) {
        const right = new THREE.Vector3().setFromMatrixColumn(camera.matrix, 0);
        const up = new THREE.Vector3().setFromMatrixColumn(camera.matrix, 1);
        target.addScaledVector(right, -dx * radius * 0.0018);
        target.addScaledVector(up, dy * radius * 0.0018);
      } else {
        theta -= dx * 0.008; phi -= dy * 0.008;
      }
      updateCamera();
    }
    function onUp() { dragging = false; }
    function onWheel(e) { e.preventDefault(); radius *= e.deltaY > 0 ? 1.09 : 0.92; updateCamera(); }
    function onContext(e) { e.preventDefault(); }
    renderer.domElement.addEventListener("pointerdown", onDown);
    renderer.domElement.addEventListener("pointermove", onMove);
    renderer.domElement.addEventListener("pointerup", onUp);
    renderer.domElement.addEventListener("pointercancel", onUp);
    renderer.domElement.addEventListener("wheel", onWheel, { passive: false });
    renderer.domElement.addEventListener("contextmenu", onContext);

    let raf = 0, disposed = false;
    function resize() {
      const w = Math.max(1, container.clientWidth), h = Math.max(1, container.clientHeight);
      renderer.setSize(w, h, false); camera.aspect = w / h; camera.updateProjectionMatrix();
    }
    const observer = new ResizeObserver(resize); observer.observe(container); resize();
    function animate() {
      if (disposed) return;
      raf = requestAnimationFrame(animate);
      renderer.render(scene, camera);
    }
    animate();
    let selectedMesh = null;
    instances.set(containerId, {
      renderer, scene, camera, root, observer, raf, overlayGroups,
      select(resourceId) {
        if (selectedMesh) {
          selectedMesh.material.emissive.setHex(0x000000);
          selectedMesh.material.emissiveIntensity = 0;
        }
        selectedMesh = nodeById.get(resourceId) || null;
        if (selectedMesh) {
          selectedMesh.material.emissive.setHex(0x60a5fa);
          selectedMesh.material.emissiveIntensity = 0.8;
        }
      },
      listeners: [
        ["pointerdown", onDown], ["pointermove", onMove], ["pointerup", onUp],
        ["pointercancel", onUp], ["wheel", onWheel], ["contextmenu", onContext]
      ],
      dispose() {
        disposed = true; cancelAnimationFrame(raf); observer.disconnect();
        this.listeners.forEach(([name, fn]) => renderer.domElement.removeEventListener(name, fn));
        scene.traverse(o => {
          if (o.geometry) o.geometry.dispose();
          if (o.material) (Array.isArray(o.material) ? o.material : [o.material]).forEach(m => m.dispose());
        });
        renderer.dispose(); renderer.domElement.remove();
      }
    });
  }
  function dispose(id) {
    const old = instances.get(id);
    if (old) { old.dispose(); instances.delete(id); }
  }
  function setSelected(id, resourceId) {
    const item = instances.get(id);
    if (item) item.select(resourceId);
  }
  function setOverlays(id, state) {
    const item = instances.get(id);
    if (!item) return;
    Object.entries({ pcie: state.pcie, gpu: state.gpu, network: state.network, storage: state.storage, power: state.power })
      .forEach(([key, visible]) => { if (item.overlayGroups[key]) item.overlayGroups[key].visible = !!visible; });
  }
  window.EfInfrastructureScene = { init, dispose, setOverlays, setSelected };
})();
