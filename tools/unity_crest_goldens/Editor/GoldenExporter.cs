using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Rendering;

namespace Crest.Goldens
{
    public static class GoldenExporter
    {
        [MenuItem("Crest/Export Water Goldens")]
        public static void ExportFromMenu()
        {
            Run(false);
        }

        public static void ExportAll()
        {
            Run(true);
        }

        static void Run(bool exitWhenDone)
        {
            if (ExportRunner.Active != null)
            {
                return;
            }

            string outputRoot;
            try
            {
                outputRoot = OutputPaths.Resolve();
            }
            catch (Exception e)
            {
                Debug.LogError("[goldens] " + e.Message);
                if (exitWhenDone) EditorApplication.Exit(1);
                return;
            }

            var runner = new ExportRunner(outputRoot, exitWhenDone);
            ExportRunner.Active = runner;
            try
            {
                runner.Start();
            }
            catch (Exception e)
            {
                Debug.LogError("[goldens] export failed: " + e);
                ExportRunner.Active = null;
                if (exitWhenDone) EditorApplication.Exit(1);
            }
        }
    }

    static class OutputPaths
    {
        public static string Resolve()
        {
            var fromEnv = Environment.GetEnvironmentVariable("CREST_GOLDENS_OUTPUT");
            if (!string.IsNullOrEmpty(fromEnv))
            {
                return fromEnv;
            }
            var args = Environment.GetCommandLineArgs();
            for (var i = 0; i < args.Length - 1; i++)
            {
                if (args[i] == "-goldenOutput")
                {
                    return args[i + 1];
                }
            }
            throw new InvalidOperationException("no output path; set CREST_GOLDENS_OUTPUT or pass -goldenOutput <path>");
        }
    }

    static class CrestDrive
    {
        static readonly MethodInfo RunUpdateMethod = typeof(OceanRenderer).GetMethod("RunUpdate", BindingFlags.NonPublic | BindingFlags.Instance);
        static readonly MethodInfo EditorUpdateMethod = typeof(OceanRenderer).GetMethod("EditorUpdate", BindingFlags.NonPublic | BindingFlags.Static);

        public static void RunOceanUpdate(OceanRenderer ocean)
        {
            if (RunUpdateMethod == null)
            {
                throw new InvalidOperationException("OceanRenderer.RunUpdate not found; expected Crest 4.23.0");
            }
            RunUpdateMethod.Invoke(ocean, null);
        }

        public static void DetachCrestEditorUpdate()
        {
            if (EditorUpdateMethod != null)
            {
                EditorApplication.update -= (EditorApplication.CallbackHandler)Delegate.CreateDelegate(typeof(EditorApplication.CallbackHandler), EditorUpdateMethod);
            }
        }

        public static void Invoke(object target, MethodInfo method)
        {
            if (method == null)
            {
                throw new InvalidOperationException("Crest callback not found; expected Crest 4.23.0");
            }
            method.Invoke(target, null);
        }
    }

    static class WorldSampling
    {
        public static int CoveringLod(OceanRenderer ocean, float x, float z)
        {
            var count = ocean.CurrentLodCount;
            for (var lod = 0; lod < count; lod++)
            {
                var rect = ocean._lodTransform._renderData[lod].Current.RectXZ;
                if (x >= rect.xMin && x <= rect.xMax && z >= rect.yMin && z <= rect.yMax)
                {
                    return lod;
                }
            }
            return count - 1;
        }

        public static void Texel(OceanRenderer ocean, int lod, float x, float z, out int col, out int row)
        {
            var renderData = ocean._lodTransform._renderData[lod].Current;
            var rect = renderData.RectXZ;
            var res = (int)renderData._textureRes;
            col = Mathf.Clamp(Mathf.RoundToInt((x - rect.xMin) / renderData._texelWidth - 0.5f), 0, res - 1);
            row = Mathf.Clamp(Mathf.RoundToInt((z - rect.yMin) / renderData._texelWidth - 0.5f), 0, res - 1);
        }
    }

    sealed class SimClock
    {
        public const float Dt = 1f / 60f;
        public float Time;
        public Action PreOcean;
        readonly ScenarioRig _rig;

        public SimClock(ScenarioRig rig)
        {
            _rig = rig;
        }

        public void Step(float dt)
        {
            _rig.timeProvider._time = Time;
            _rig.timeProvider._deltaTime = dt;
            if (dt > 0f && PreOcean != null)
            {
                PreOcean();
            }
            CrestDrive.RunOceanUpdate(_rig.ocean);
            Time += dt;
        }
    }

    sealed class ScenarioContext
    {
        public ScenarioSpec Spec;
        public ScenarioRig Rig;
        public SimClock Clock;
        public GoldenDocumentDto Doc;
        public string Dir;
        public TrajectoryDto Trajectory;
        public float SampleInterval;
        public float NextSampleTime;

        public OceanRenderer Ocean
        {
            get { return Rig.ocean; }
        }

        public IFlowProvider FlowProvider
        {
            get { return Spec.flowEnabled ? Rig.ocean.FlowProvider : null; }
        }

        public void AddTrajectorySample(float t)
        {
            var position = Rig.boatBody.transform.position;
            var rotation = Rig.boatBody.transform.rotation;
            Trajectory.samples.Add(new TrajectorySampleDto
            {
                t = t,
                position = new[] { (double)position.x, (double)position.y, (double)position.z },
                rotationXyzw = new[] { (double)rotation.x, (double)rotation.y, (double)rotation.z, (double)rotation.w },
            });
        }
    }

    interface IWorkItem
    {
        bool Tick();
    }

    sealed class BuildWork : IWorkItem
    {
        readonly ScenarioContext _ctx;

        public BuildWork(ScenarioContext ctx)
        {
            _ctx = ctx;
        }

        public bool Tick()
        {
            Debug.Log("[goldens] building " + _ctx.Spec.name);
            _ctx.Rig = ScenarioBuilder.Build(_ctx.Spec);
            _ctx.Clock = new SimClock(_ctx.Rig);
            CrestDrive.DetachCrestEditorUpdate();
            _ctx.Clock.Step(0f);

            var ocean = _ctx.Rig.ocean;
            if (ocean._lodDataAnimWaves == null)
            {
                throw new InvalidOperationException("animated waves simulation missing; run with a graphics device and without -nographics");
            }
            if (_ctx.Spec.foamEnabled && ocean._lodDataFoam == null)
            {
                throw new InvalidOperationException("Crest does not create the foam simulation in batch mode; run foam scenarios without -batchmode");
            }
            if (_ctx.Spec.dynamicWavesEnabled && ocean._lodDataDynWaves == null)
            {
                throw new InvalidOperationException("dynamic wave simulation missing");
            }
            if (_ctx.Spec.flowEnabled && ocean._lodDataFlow == null)
            {
                throw new InvalidOperationException("flow simulation missing");
            }
            if (_ctx.Spec.foamEnabled)
            {
                ocean._lodDataFoam.Settings._simulationFrequency = 60f;
            }
            if (_ctx.Spec.dynamicWavesEnabled)
            {
                ocean._lodDataDynWaves.Settings._simulationFrequency = 60f;
            }

            var texel = ocean._lodTransform._renderData[0].Current._texelWidth;
            if (Mathf.Abs(texel - _ctx.Spec.baseTexelSize) > 1e-3f)
            {
                throw new InvalidOperationException("lod0 texel width " + texel + " does not match base_texel_size " + _ctx.Spec.baseTexelSize);
            }

            if (_ctx.Spec.boat != null)
            {
                var boat = _ctx.Spec.boat;
                var rig = _ctx.Rig;
                _ctx.Clock.PreOcean = () =>
                {
                    CrestDrive.Invoke(rig.boatProbes, rig.boatFixedUpdate);
                    rig.boatBody.AddForce(rig.boatBody.transform.forward * boat.thrustNewtons, ForceMode.Force);
                    rig.boatBody.AddTorque(Vector3.up * boat.yawTorqueNewtonMeters, ForceMode.Force);
                    Physics.Simulate(SimClock.Dt);
                    foreach (var sphere in rig.hullSpheres)
                    {
                        CrestDrive.Invoke(sphere, rig.sphereLateUpdate);
                    }
                };
            }
            return true;
        }
    }

    sealed class TrajectorySampleWork : IWorkItem
    {
        readonly ScenarioContext _ctx;
        readonly float _t;

        public TrajectorySampleWork(ScenarioContext ctx, float t)
        {
            _ctx = ctx;
            _t = t;
        }

        public bool Tick()
        {
            _ctx.AddTrajectorySample(_t);
            return true;
        }
    }

    sealed class StepToWork : IWorkItem
    {
        const float Eps = 1e-3f;
        readonly ScenarioContext _ctx;
        readonly float _target;

        public StepToWork(ScenarioContext ctx, float target)
        {
            _ctx = ctx;
            _target = target;
        }

        public bool Tick()
        {
            for (var i = 0; i < 8 && _ctx.Clock.Time < _target - Eps; i++)
            {
                _ctx.Clock.Step(SimClock.Dt);
                if (_ctx.Trajectory != null && _ctx.Clock.Time >= _ctx.NextSampleTime - Eps)
                {
                    _ctx.AddTrajectorySample(_ctx.NextSampleTime);
                    _ctx.NextSampleTime += _ctx.SampleInterval;
                }
            }
            return _ctx.Clock.Time >= _target - Eps;
        }
    }

    sealed class CheckpointWork : IWorkItem
    {
        enum Phase
        {
            Prime,
            Settle,
            Request,
            Poll,
            Harvest
        }

        readonly ScenarioContext _ctx;
        readonly ProbeGroupSpec _group;
        readonly SliceSpec[] _exportSlices;
        readonly Vector3[] _points;
        readonly Vector3[] _disps;
        readonly Vector3[] _norms;
        readonly Vector3[] _flows;
        readonly int _collHash;
        readonly int _flowHash;
        readonly List<SliceCapture> _captures = new List<SliceCapture>();
        readonly Dictionary<SliceSpec, SliceCapture> _exportCapture = new Dictionary<SliceSpec, SliceCapture>();
        readonly Dictionary<int, SliceCapture> _probeFoam = new Dictionary<int, SliceCapture>();
        readonly Dictionary<int, SliceCapture> _probeDepth = new Dictionary<int, SliceCapture>();
        readonly Dictionary<SliceCapture, float[]> _extracted = new Dictionary<SliceCapture, float[]>();
        Phase _phase;
        int _ticks;
        bool _collOk;
        bool _flowOk;

        public CheckpointWork(ScenarioContext ctx, ProbeGroupSpec group)
        {
            _ctx = ctx;
            _group = group;

            var slices = new List<SliceSpec>();
            foreach (var slice in ctx.Spec.slices)
            {
                if (slice.time == (int)group.time)
                {
                    slices.Add(slice);
                }
            }
            _exportSlices = slices.ToArray();

            var count = group.probes.Length;
            _points = new Vector3[count];
            _disps = new Vector3[count];
            _norms = new Vector3[count];
            _flows = new Vector3[count];
            for (var i = 0; i < count; i++)
            {
                _points[i] = new Vector3(group.probes[i].x, ctx.Spec.waterLevel, group.probes[i].z);
            }
            _collHash = StableHash(ctx.Spec.name + "_coll_" + (int)group.time);
            _flowHash = StableHash(ctx.Spec.name + "_flow_" + (int)group.time);
        }

        public bool Tick()
        {
            switch (_phase)
            {
                case Phase.Prime:
                    _ctx.Clock.Time = _group.time;
                    PostQueries();
                    _ctx.Clock.Step(0f);
                    _phase = Phase.Settle;
                    return false;

                case Phase.Settle:
                    _ctx.Clock.Step(0f);
                    _ticks++;
                    if (_ticks >= 3)
                    {
                        _phase = Phase.Request;
                    }
                    return false;

                case Phase.Request:
                    IssueCaptures();
                    _ticks = 0;
                    _phase = Phase.Poll;
                    return false;

                case Phase.Poll:
                    _ctx.Clock.Step(0f);
                    _ticks++;
                    PostQueries();
                    if (_ticks > 180)
                    {
                        throw new TimeoutException("checkpoint at t=" + _group.time + " timed out waiting for queries or readbacks");
                    }
                    if (!AllCapturesDone() || !(_collOk && _flowOk))
                    {
                        return false;
                    }
                    _phase = Phase.Harvest;
                    return Harvest();

                default:
                    return false;
            }
        }

        void PostQueries()
        {
            var coll = _ctx.Ocean.CollisionProvider;
            if (coll == null)
            {
                throw new InvalidOperationException("collision provider missing");
            }
            var status = coll.Query(_collHash, 0f, _points, _disps, _norms, null);
            if (coll.RetrieveSucceeded(status))
            {
                _collOk = true;
            }

            var flow = _ctx.FlowProvider;
            if (flow == null)
            {
                _flowOk = true;
                return;
            }
            var flowStatus = flow.Query(_flowHash, 0f, _points, _flows);
            if (flow.RetrieveSucceeded(flowStatus))
            {
                _flowOk = true;
            }
        }

        void IssueCaptures()
        {
            foreach (var slice in _exportSlices)
            {
                var capture = SliceWriter.Begin(ManagerFor(slice.channel), slice.lod, slice.channel, slice.name, slice.name + ".bin");
                _exportCapture[slice] = capture;
                _captures.Add(capture);
            }

            var lods = new HashSet<int>();
            foreach (var probe in _group.probes)
            {
                lods.Add(WorldSampling.CoveringLod(_ctx.Ocean, probe.x, probe.z));
            }
            foreach (var lod in lods)
            {
                if (_ctx.Spec.foamEnabled)
                {
                    var foam = SliceWriter.Begin(_ctx.Ocean._lodDataFoam, lod, SliceChannel.Foam, "probe_foam_lod" + lod, null);
                    _probeFoam[lod] = foam;
                    _captures.Add(foam);
                }
                var depth = SliceWriter.Begin(_ctx.Ocean._lodDataSeaDepths, lod, SliceChannel.SeaFloorDepth, "probe_depth_lod" + lod, null);
                _probeDepth[lod] = depth;
                _captures.Add(depth);
            }
        }

        bool AllCapturesDone()
        {
            foreach (var capture in _captures)
            {
                if (!SliceWriter.IsDone(capture))
                {
                    return false;
                }
            }
            return true;
        }

        bool Harvest()
        {
            foreach (var capture in _captures)
            {
                _extracted[capture] = SliceWriter.Extract(capture);
            }

            var seaLevel = _ctx.Ocean.SeaLevel;
            for (var i = 0; i < _group.probes.Length; i++)
            {
                var probe = _group.probes[i];
                var lod = WorldSampling.CoveringLod(_ctx.Ocean, probe.x, probe.z);
                WorldSampling.Texel(_ctx.Ocean, lod, probe.x, probe.z, out var col, out var row);
                var disp = _disps[i];
                var norm = _norms[i];
                var dto = new ProbeDto
                {
                    name = probe.name,
                    x = probe.x,
                    z = probe.z,
                    t = _group.time,
                    lod = lod,
                    height = seaLevel + disp.y,
                    displacement = new[] { (double)disp.x, (double)disp.y, (double)disp.z },
                    normal = new[] { (double)norm.x, (double)norm.y, (double)norm.z },
                    flow = _ctx.Spec.flowEnabled
                        ? new[] { (double)_flows[i].x, (double)_flows[i].z }
                        : new[] { 0.0, 0.0 },
                };
                if (_probeFoam.TryGetValue(lod, out var foamCapture))
                {
                    dto.foam = SliceWriter.SamplePoint(foamCapture, _extracted[foamCapture], col, row);
                }
                if (_probeDepth.TryGetValue(lod, out var depthCapture))
                {
                    dto.seaFloorDepth = SliceWriter.SamplePoint(depthCapture, _extracted[depthCapture], col, row);
                }
                _ctx.Doc.probes.Add(dto);
            }

            foreach (var slice in _exportSlices)
            {
                var capture = _exportCapture[slice];
                var data = _extracted[capture];
                var file = slice.name + ".bin";
                SliceWriter.WriteBin(Path.Combine(_ctx.Dir, file), data);
                _ctx.Doc.slices.Add(new SliceDto
                {
                    name = slice.name,
                    lod = slice.lod,
                    t = slice.time,
                    channels = SliceWriter.ChannelsLabel(slice.channel),
                    width = capture.width,
                    height = capture.height,
                    file = file,
                });
            }
            return true;
        }

        LodDataMgr ManagerFor(SliceChannel channel)
        {
            switch (channel)
            {
                case SliceChannel.DisplacementXyzJac: return _ctx.Ocean._lodDataAnimWaves;
                case SliceChannel.FlowXz: return _ctx.Ocean._lodDataFlow;
                case SliceChannel.Foam: return _ctx.Ocean._lodDataFoam;
                case SliceChannel.SeaFloorDepth: return _ctx.Ocean._lodDataSeaDepths;
                default: throw new InvalidOperationException("unknown slice channel");
            }
        }

        static int StableHash(string value)
        {
            unchecked
            {
                var hash = 17;
                foreach (var c in value)
                {
                    hash = hash * 31 + c;
                }
                return hash;
            }
        }
    }

    sealed class FrameWork : IWorkItem
    {
        readonly ScenarioContext _ctx;
        readonly FrameSpec _spec;

        public FrameWork(ScenarioContext ctx, FrameSpec spec)
        {
            _ctx = ctx;
            _spec = spec;
        }

        public bool Tick()
        {
            var go = new GameObject("GoldenFrameCamera");
            var cam = go.AddComponent<Camera>();
            cam.transform.position = _spec.position;
            cam.transform.rotation = _spec.rotation;
            cam.fieldOfView = _spec.fov;
            cam.clearFlags = CameraClearFlags.SolidColor;
            cam.backgroundColor = Color.black;
            cam.nearClipPlane = 0.1f;
            cam.farClipPlane = 4000f;
            cam.cullingMask = ~0;

            var rt = new RenderTexture(_spec.width, _spec.height, 24);
            rt.Create();
            cam.targetTexture = rt;
            cam.Render();

            var previous = RenderTexture.active;
            RenderTexture.active = rt;
            var tex = new Texture2D(_spec.width, _spec.height, TextureFormat.RGBA32, false);
            tex.ReadPixels(new Rect(0f, 0f, _spec.width, _spec.height), 0, 0);
            tex.Apply();
            RenderTexture.active = previous;

            var file = _spec.name + ".png";
            File.WriteAllBytes(Path.Combine(_ctx.Dir, file), tex.EncodeToPNG());

            var position = _spec.position;
            var rotation = _spec.rotation;
            _ctx.Doc.frames.Add(new FrameDto
            {
                name = _spec.name,
                file = file,
                camera = new CameraDto
                {
                    position = new[] { (double)position.x, (double)position.y, (double)position.z },
                    rotationXyzw = new[] { (double)rotation.x, (double)rotation.y, (double)rotation.z, (double)rotation.w },
                    fov = _spec.fov,
                },
            });

            UnityEngine.Object.DestroyImmediate(tex);
            UnityEngine.Object.DestroyImmediate(rt);
            UnityEngine.Object.DestroyImmediate(go);
            return true;
        }
    }

    sealed class WriteJsonWork : IWorkItem
    {
        readonly ScenarioContext _ctx;

        public WriteJsonWork(ScenarioContext ctx)
        {
            _ctx = ctx;
        }

        public bool Tick()
        {
            var path = Path.Combine(_ctx.Dir, "golden.json");
            File.WriteAllText(path, GoldenJson.Write(_ctx.Doc));
            Debug.Log("[goldens] wrote " + path);
            return true;
        }
    }

    sealed class TeardownWork : IWorkItem
    {
        readonly ScenarioContext _ctx;

        public TeardownWork(ScenarioContext ctx)
        {
            _ctx = ctx;
        }

        public bool Tick()
        {
            ScenarioBuilder.Teardown(_ctx.Rig);
            return true;
        }
    }

    public sealed class ExportRunner
    {
        public static ExportRunner Active;

        readonly string _outputRoot;
        readonly bool _exitWhenDone;
        readonly List<IWorkItem> _works = new List<IWorkItem>();
        int _index;
        bool _finished;
        RenderTexture _pumpTarget;
        Texture2D _pumpSource;

        public ExportRunner(string outputRoot, bool exitWhenDone)
        {
            _outputRoot = outputRoot;
            _exitWhenDone = exitWhenDone;
        }

        public void Start()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
            if (OceanRenderer.RunningWithoutGPU)
            {
                throw new InvalidOperationException("no graphics device; run without -nographics");
            }
            foreach (var spec in ScenarioBuilder.AllScenarios())
            {
                BeginScenario(spec);
            }
            EditorApplication.update += OnUpdate;
            OnUpdate();
        }

        void BeginScenario(ScenarioSpec spec)
        {
            var ctx = new ScenarioContext
            {
                Spec = spec,
                Dir = Path.Combine(_outputRoot, spec.name),
                Doc = new GoldenDocumentDto
                {
                    settings = BuildSettings(spec),
                    tolerances = new TolerancesDto
                    {
                        height = 0.02,
                        normal = 0.05,
                        flow = 0.05,
                        foam = 0.15,
                        depth = 0.05,
                        sliceRms = 0.01,
                        trajectory = 0.1,
                    },
                },
            };
            Directory.CreateDirectory(ctx.Dir);

            _works.Add(new BuildWork(ctx));

            if (spec.boat != null)
            {
                ctx.Trajectory = new TrajectoryDto { name = spec.boat.trajectoryName };
                ctx.SampleInterval = spec.boat.sampleIntervalSeconds;
                ctx.NextSampleTime = ctx.SampleInterval;
                ctx.Doc.trajectories.Add(ctx.Trajectory);
                _works.Add(new TrajectorySampleWork(ctx, 0f));
            }

            foreach (var group in spec.probeGroups)
            {
                _works.Add(new StepToWork(ctx, group.time));
                _works.Add(new CheckpointWork(ctx, group));
            }

            foreach (var frame in spec.frames)
            {
                _works.Add(new FrameWork(ctx, frame));
            }

            _works.Add(new WriteJsonWork(ctx));
            _works.Add(new TeardownWork(ctx));
        }

        void OnUpdate()
        {
            if (_finished)
            {
                return;
            }
            try
            {
                PumpGpu();
                if (_works[_index].Tick())
                {
                    _index++;
                    if (_index >= _works.Count)
                    {
                        Finish(0);
                    }
                }
            }
            catch (Exception e)
            {
                Debug.LogError("[goldens] export failed: " + e);
                Finish(1);
            }
        }

        void PumpGpu()
        {
            if (_pumpTarget == null)
            {
                _pumpTarget = new RenderTexture(1, 1, 0);
                _pumpSource = new Texture2D(1, 1, TextureFormat.RGBA32, false);
            }
            Graphics.Blit(_pumpSource, _pumpTarget);
        }

        void Finish(int code)
        {
            if (_finished)
            {
                return;
            }
            _finished = true;
            EditorApplication.update -= OnUpdate;
            Active = null;
            Debug.Log("[goldens] export finished with code " + code);
            if (_exitWhenDone)
            {
                EditorApplication.Exit(code);
            }
        }

        static GoldenSettingsDto BuildSettings(ScenarioSpec spec)
        {
            var settings = new GoldenSettingsDto
            {
                waterLevel = spec.waterLevel,
                windSpeed = spec.windSpeed,
                windDirX = spec.windDirection.x,
                windDirZ = spec.windDirection.y,
                gravity = spec.gravity,
                chop = spec.chop,
                turbulence = spec.turbulence,
                amplitudeMultiplier = spec.amplitudeMultiplier,
                loopPeriod = spec.loopPeriod,
                lodCount = spec.lodCount,
                lodResolution = spec.lodResolution,
                baseTexelSize = spec.baseTexelSize,
                seabed = BuildSeabedDto(spec.seabed),
            };
            foreach (var spline in spec.splines)
            {
                var dto = new SplineDto
                {
                    width = spline.width,
                    flowSpeed = spline.flowSpeed,
                    waveMultiplier = spline.waveMultiplier,
                    foamAmount = spline.foamAmount,
                };
                foreach (var point in spline.points)
                {
                    dto.points.Add(new[] { (double)point.x, (double)point.y, (double)point.z });
                }
                settings.splines.Add(dto);
            }
            return settings;
        }

        static SeabedDto BuildSeabedDto(SeabedSpec seabed)
        {
            if (seabed.kind == SeabedKind.Flat)
            {
                return new SeabedDto { kind = "flat", height = seabed.height };
            }
            return new SeabedDto
            {
                kind = "slope",
                baseHeight = seabed.baseHeight,
                slopeX = seabed.slopeX,
                slopeZ = seabed.slopeZ,
                min = seabed.min,
                max = seabed.max,
            };
        }
    }
}
