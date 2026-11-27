using System;
using System.Collections.Generic;
using System.Globalization;
using System.Reflection;
using Crest.Spline;
using UnityEditor;
using UnityEngine;

namespace Crest.Goldens
{
    public enum SeabedKind
    {
        Flat,
        Slope
    }

    public sealed class SeabedSpec
    {
        public SeabedKind kind;
        public float height;
        public float baseHeight;
        public float slopeX;
        public float slopeZ;
        public float min;
        public float max;
    }

    public sealed class SplineSpec
    {
        public Vector3[] points;
        public float width;
        public float flowSpeed;
        public float waveMultiplier;
        public float foamAmount;
    }

    public sealed class ProbeSpec
    {
        public string name;
        public float x;
        public float z;
    }

    public sealed class ProbeGroupSpec
    {
        public float time;
        public ProbeSpec[] probes;
    }

    public sealed class SliceSpec
    {
        public SliceChannel channel;
        public int lod;
        public int time;
        public string name;
    }

    public sealed class FrameSpec
    {
        public string name;
        public Vector3 position;
        public Quaternion rotation;
        public float fov;
        public int width;
        public int height;
    }

    public sealed class BoatSpec
    {
        public string trajectoryName = "boat";
        public Vector3 start;
        public float headingDegrees;
        public Vector3 hullSize = new Vector3(2f, 0.5f, 4f);
        public float mass = 800f;
        public float thrustNewtons = 600f;
        public float yawTorqueNewtonMeters = 40f;
        public float durationSeconds = 10f;
        public float sampleIntervalSeconds = 0.5f;
        public ProbeGroupSpec[] wakeProbeGroups = new ProbeGroupSpec[0];
    }

    public sealed class ScenarioSpec
    {
        public string name;
        public float waterLevel;
        public float windSpeed;
        public Vector2 windDirection;
        public float gravity = 9.81f;
        public float chop = 1.6f;
        public float turbulence = 0.145f;
        public float amplitudeMultiplier = 1f;
        public float loopPeriod = 32f;
        public int lodCount = 7;
        public int lodResolution = 64;
        public float baseTexelSize = 0.5f;
        public SeabedSpec seabed;
        public SplineSpec[] splines = new SplineSpec[0];
        public bool flowEnabled;
        public bool foamEnabled;
        public bool dynamicWavesEnabled;
        public ProbeGroupSpec[] probeGroups = new ProbeGroupSpec[0];
        public SliceSpec[] slices = new SliceSpec[0];
        public FrameSpec[] frames = new FrameSpec[0];
        public BoatSpec boat;
    }

    public sealed class ScenarioRig
    {
        public OceanRenderer ocean;
        public Camera camera;
        public TimeProviderCustom timeProvider;
        public OceanWaveSpectrum spectrum;
        public Rigidbody boatBody;
        public BoatProbes boatProbes;
        public SphereWaterInteraction[] hullSpheres = new SphereWaterInteraction[0];
        public MethodInfo boatFixedUpdate;
        public MethodInfo sphereLateUpdate;
        public List<GameObject> roots = new List<GameObject>();
    }

    public static class ScenarioBuilder
    {
        public static List<ScenarioSpec> AllScenarios()
        {
            return new List<ScenarioSpec>
            {
                CalmOcean(),
                StormSwell(),
                ShallowShore(),
                SplineRiver(),
                BoatWakes(),
                UnderwaterCurtain(),
            };
        }

        public static ScenarioRig Build(ScenarioSpec spec)
        {
            var rig = new ScenarioRig();

            Time.fixedDeltaTime = 1f / 60f;
            Physics.autoSimulation = false;
            Physics.gravity = new Vector3(0f, -spec.gravity, 0f);

            var oceanGo = new GameObject("GoldenOcean");
            oceanGo.transform.position = new Vector3(0f, spec.waterLevel, 0f);
            var ocean = oceanGo.AddComponent<OceanRenderer>();
            rig.roots.Add(oceanGo);
            rig.ocean = ocean;

            var so = new SerializedObject(ocean);
            Prop(so, "_lodDataResolution").intValue = spec.lodResolution;
            Prop(so, "_lodCount").intValue = spec.lodCount;
            Prop(so, "_minScale").floatValue = spec.baseTexelSize * spec.lodResolution / 4f;
            Prop(so, "_followSceneCamera").boolValue = false;
            Prop(so, "_createSeaFloorDepthData").boolValue = true;
            Prop(so, "_createFoamSim").boolValue = spec.foamEnabled;
            Prop(so, "_createFlowSim").boolValue = spec.flowEnabled;
            Prop(so, "_createDynamicWaveSim").boolValue = spec.dynamicWavesEnabled;
            so.ApplyModifiedProperties();

            ocean._globalWindSpeed = spec.windSpeed * 3.6f;
            ocean._globalWindDirectionAngle = Mathf.Atan2(spec.windDirection.y, spec.windDirection.x) * Mathf.Rad2Deg;
            ocean._globalWindTurbulence = spec.turbulence;
            ocean.OceanMaterial = LoadOceanMaterial();

            var cameraGo = new GameObject("GoldenCamera");
            cameraGo.transform.position = new Vector3(0f, 2f, 0f);
            var cam = cameraGo.AddComponent<Camera>();
            cam.clearFlags = CameraClearFlags.SolidColor;
            cam.backgroundColor = Color.black;
            cam.nearClipPlane = 0.1f;
            cam.farClipPlane = 4000f;
            cam.cullingMask = ~0;
            ocean.ViewCamera = cam;
            ocean.Viewpoint = cameraGo.transform;
            rig.roots.Add(cameraGo);
            rig.camera = cam;

            var timeGo = new GameObject("GoldenTime");
            var timeProvider = timeGo.AddComponent<TimeProviderCustom>();
            timeProvider._overrideTime = true;
            timeProvider._time = 0f;
            timeProvider._overrideDeltaTime = true;
            timeProvider._deltaTime = 0f;
            ocean.PushTimeProvider(timeProvider);
            rig.roots.Add(timeGo);
            rig.timeProvider = timeProvider;

            rig.spectrum = ScriptableObject.CreateInstance<OceanWaveSpectrum>();
            rig.spectrum._chop = spec.chop;
            rig.spectrum._multiplier = spec.amplitudeMultiplier;

            var shapeGo = new GameObject("GoldenShape");
            var shape = shapeGo.AddComponent<ShapeFFT>();
            shape._spectrum = rig.spectrum;
            shape._timeLoopLength = spec.loopPeriod;
            rig.roots.Add(shapeGo);

            rig.roots.Add(BuildSeabed(spec.seabed));

            foreach (var spline in spec.splines)
            {
                rig.roots.Add(BuildSpline(spline, rig.spectrum, spec.loopPeriod));
            }

            if (spec.boat != null)
            {
                BuildBoat(spec.boat, rig);
            }

            return rig;
        }

        public static void Teardown(ScenarioRig rig)
        {
            foreach (var root in rig.roots)
            {
                if (root != null) UnityEngine.Object.DestroyImmediate(root);
            }
            if (rig.spectrum != null) UnityEngine.Object.DestroyImmediate(rig.spectrum);
        }

        static ScenarioSpec CalmOcean()
        {
            var xs = new[] { -12f, -4f, 4f, 12f };
            var zs = new[] { -12f, -4f, 4f, 12f };
            return new ScenarioSpec
            {
                name = "calm_ocean",
                waterLevel = 0f,
                windSpeed = 4f,
                windDirection = new Vector2(1f, 0f),
                seabed = FlatSeabed(-20f),
                probeGroups = new[] { GridGroup(0f, xs, zs), GridGroup(4f, xs, zs), GridGroup(8f, xs, zs) },
                slices = new[]
                {
                    new SliceSpec { channel = SliceChannel.DisplacementXyzJac, lod = 0, time = 4, name = "disp_lod0_t4" },
                    new SliceSpec { channel = SliceChannel.SeaFloorDepth, lod = 0, time = 0, name = "depth_lod0_t0" },
                },
            };
        }

        static ScenarioSpec StormSwell()
        {
            var xs = new[] { -12f, -4f, 4f, 12f };
            var zs = new[] { -12f, -4f, 4f, 12f };
            return new ScenarioSpec
            {
                name = "storm_swell",
                waterLevel = 0f,
                windSpeed = 20f,
                windDirection = new Vector2(0.707f, 0.707f),
                turbulence = 0.4f,
                seabed = FlatSeabed(-20f),
                foamEnabled = true,
                probeGroups = new[] { GridGroup(0f, xs, zs), GridGroup(4f, xs, zs), GridGroup(8f, xs, zs) },
                slices = new[]
                {
                    new SliceSpec { channel = SliceChannel.DisplacementXyzJac, lod = 0, time = 4, name = "disp_lod0_t4" },
                    new SliceSpec { channel = SliceChannel.Foam, lod = 0, time = 8, name = "foam_lod0_t8" },
                },
            };
        }

        static ScenarioSpec ShallowShore()
        {
            var xs = new[] { -20f, -10f, -4f, -1f, 0f, 2f, 6f };
            var zs = new[] { 0f };
            return new ScenarioSpec
            {
                name = "shallow_shore",
                waterLevel = 0f,
                windSpeed = 8f,
                windDirection = new Vector2(1f, 0f),
                seabed = new SeabedSpec
                {
                    kind = SeabedKind.Slope,
                    baseHeight = -6f,
                    slopeX = 0.12f,
                    slopeZ = 0f,
                    min = -6f,
                    max = 2f,
                },
                foamEnabled = true,
                probeGroups = new[] { GridGroup(0f, xs, zs), GridGroup(4f, xs, zs) },
                slices = new[]
                {
                    new SliceSpec { channel = SliceChannel.SeaFloorDepth, lod = 0, time = 0, name = "depth_lod0_t0" },
                    new SliceSpec { channel = SliceChannel.Foam, lod = 0, time = 4, name = "foam_lod0_t4" },
                },
            };
        }

        static ScenarioSpec SplineRiver()
        {
            return new ScenarioSpec
            {
                name = "spline_river",
                waterLevel = 0f,
                windSpeed = 4f,
                windDirection = new Vector2(0f, 1f),
                seabed = FlatSeabed(-10f),
                flowEnabled = true,
                foamEnabled = true,
                splines = new[]
                {
                    new SplineSpec
                    {
                        points = new[]
                        {
                            new Vector3(0f, -1.5f, -60f),
                            new Vector3(0f, -1.5f, -20f),
                            new Vector3(0f, -1.2f, 20f),
                            new Vector3(0f, -1.0f, 60f),
                        },
                        width = 8f,
                        flowSpeed = 1.5f,
                        waveMultiplier = 0.3f,
                        foamAmount = 0.2f,
                    },
                },
                probeGroups = new[] { RiverGroup(0f), RiverGroup(4f) },
                slices = new[]
                {
                    new SliceSpec { channel = SliceChannel.FlowXz, lod = 0, time = 4, name = "flow_lod0_t4" },
                },
            };
        }

        static ScenarioSpec BoatWakes()
        {
            return new ScenarioSpec
            {
                name = "boat_wakes",
                waterLevel = 0f,
                windSpeed = 6f,
                windDirection = new Vector2(1f, 0f),
                seabed = FlatSeabed(-15f),
                dynamicWavesEnabled = true,
                boat = new BoatSpec
                {
                    start = new Vector3(-8f, 0f, 0f),
                    headingDegrees = 90f,
                    wakeProbeGroups = new[] { WakeGroup(5f), WakeGroup(10f) },
                },
                probeGroups = new[] { WakeGroup(5f), WakeGroup(10f) },
            };
        }

        static ScenarioSpec UnderwaterCurtain()
        {
            return new ScenarioSpec
            {
                name = "underwater_curtain",
                waterLevel = 0f,
                windSpeed = 6f,
                windDirection = new Vector2(1f, 0f),
                seabed = FlatSeabed(-20f),
                probeGroups = new[] { CurtainGroup(0f), CurtainGroup(4f) },
                frames = new[]
                {
                    new FrameSpec
                    {
                        name = "surface_straddle",
                        position = new Vector3(0f, 0f, -8f),
                        rotation = Quaternion.identity,
                        fov = 60f,
                        width = 1024,
                        height = 512,
                    },
                },
            };
        }

        static SeabedSpec FlatSeabed(float height)
        {
            return new SeabedSpec { kind = SeabedKind.Flat, height = height };
        }

        static ProbeGroupSpec GridGroup(float t, float[] xs, float[] zs)
        {
            var probes = new List<ProbeSpec>();
            foreach (var z in zs)
            {
                foreach (var x in xs)
                {
                    probes.Add(new ProbeSpec { name = "x" + Num(x) + "_z" + Num(z) + "_t" + Num(t), x = x, z = z });
                }
            }
            return new ProbeGroupSpec { time = t, probes = probes.ToArray() };
        }

        static ProbeGroupSpec RiverGroup(float t)
        {
            var probes = new List<ProbeSpec>();
            var zs = new[] { -40f, 0f, 40f };
            foreach (var z in zs)
            {
                probes.Add(new ProbeSpec { name = "c_z" + Num(z) + "_t" + Num(t), x = 0f, z = z });
            }
            foreach (var z in zs)
            {
                foreach (var x in new[] { -6f, 6f })
                {
                    probes.Add(new ProbeSpec { name = "b_x" + Num(x) + "_z" + Num(z) + "_t" + Num(t), x = x, z = z });
                }
            }
            return new ProbeGroupSpec { time = t, probes = probes.ToArray() };
        }

        static ProbeGroupSpec WakeGroup(float t)
        {
            var points = new[]
            {
                new Vector2(6f, 0f),
                new Vector2(10f, 3f),
                new Vector2(10f, -3f),
                new Vector2(14f, 6f),
                new Vector2(14f, -6f),
                new Vector2(16f, 0f),
            };
            var probes = new List<ProbeSpec>();
            for (var i = 0; i < points.Length; i++)
            {
                probes.Add(new ProbeSpec { name = "w" + i + "_t" + Num(t), x = points[i].x, z = points[i].y });
            }
            return new ProbeGroupSpec { time = t, probes = probes.ToArray() };
        }

        static ProbeGroupSpec CurtainGroup(float t)
        {
            var probes = new List<ProbeSpec>();
            foreach (var x in new[] { -4f, 0f, 4f })
            {
                foreach (var offset in new[] { 0.3f, -0.3f })
                {
                    probes.Add(new ProbeSpec { name = "s_x" + Num(x) + "_h" + Num(offset) + "_t" + Num(t), x = x, z = 0f });
                }
            }
            return new ProbeGroupSpec { time = t, probes = probes.ToArray() };
        }

        static string Num(float value)
        {
            return value.ToString("0.###", CultureInfo.InvariantCulture);
        }

        static SerializedProperty Prop(SerializedObject serialized, string name)
        {
            var property = serialized.FindProperty(name);
            if (property == null)
            {
                throw new InvalidOperationException("Crest serialized property '" + name + "' not found; expected Crest 4.23.0");
            }
            return property;
        }

        static Material LoadOceanMaterial()
        {
            var material = AssetDatabase.LoadAssetAtPath<Material>("Assets/Crest/Crest/Materials/Ocean.mat");
            if (material != null)
            {
                return material;
            }
            foreach (var guid in AssetDatabase.FindAssets("Ocean t:Material"))
            {
                var path = AssetDatabase.GUIDToAssetPath(guid);
                if (path.IndexOf("Crest", StringComparison.OrdinalIgnoreCase) >= 0)
                {
                    material = AssetDatabase.LoadAssetAtPath<Material>(path);
                    if (material != null)
                    {
                        return material;
                    }
                }
            }
            Debug.LogWarning("[goldens] no Crest ocean material found, ocean tiles will render without a material");
            return null;
        }

        static GameObject BuildSeabed(SeabedSpec seabed)
        {
            var go = new GameObject("GoldenSeabed");
            var mesh = seabed.kind == SeabedKind.Flat
                ? FlatSeabedMesh(seabed.height)
                : SlopeSeabedMesh(seabed.baseHeight, seabed.slopeX, seabed.slopeZ, seabed.min, seabed.max);
            var filter = go.AddComponent<MeshFilter>();
            filter.sharedMesh = mesh;
            go.AddComponent<MeshRenderer>();
            go.AddComponent<RegisterSeaFloorDepthInput>();
            return go;
        }

        static Mesh FlatSeabedMesh(float height)
        {
            const float extent = 2048f;
            var mesh = new Mesh();
            mesh.vertices = new[]
            {
                new Vector3(-extent, height, -extent),
                new Vector3(extent, height, -extent),
                new Vector3(extent, height, extent),
                new Vector3(-extent, height, extent),
            };
            mesh.triangles = new[] { 0, 1, 2, 0, 2, 3, 0, 2, 1, 0, 3, 2 };
            mesh.RecalculateBounds();
            mesh.RecalculateNormals();
            return mesh;
        }

        static Mesh SlopeSeabedMesh(float baseHeight, float slopeX, float slopeZ, float min, float max)
        {
            const float extent = 2048f;
            const int cells = 32;
            var verts = new Vector3[(cells + 1) * (cells + 1)];
            for (var iz = 0; iz <= cells; iz++)
            {
                for (var ix = 0; ix <= cells; ix++)
                {
                    var x = -extent + 2f * extent * ix / cells;
                    var z = -extent + 2f * extent * iz / cells;
                    var y = Mathf.Clamp(baseHeight + slopeX * x + slopeZ * z, min, max);
                    verts[iz * (cells + 1) + ix] = new Vector3(x, y, z);
                }
            }
            var tris = new List<int>();
            for (var iz = 0; iz < cells; iz++)
            {
                for (var ix = 0; ix < cells; ix++)
                {
                    var a = iz * (cells + 1) + ix;
                    var b = a + 1;
                    var c = a + (cells + 1);
                    var d = c + 1;
                    tris.Add(a); tris.Add(b); tris.Add(c);
                    tris.Add(b); tris.Add(d); tris.Add(c);
                    tris.Add(a); tris.Add(c); tris.Add(b);
                    tris.Add(b); tris.Add(c); tris.Add(d);
                }
            }
            var mesh = new Mesh();
            mesh.vertices = verts.ToArray();
            mesh.triangles = tris.ToArray();
            mesh.RecalculateBounds();
            mesh.RecalculateNormals();
            return mesh;
        }

        static GameObject BuildSpline(SplineSpec spline, OceanWaveSpectrum spectrum, float loopPeriod)
        {
            var go = new GameObject("GoldenSpline");
            var splineComponent = go.AddComponent<Spline>();

            foreach (var point in spline.points)
            {
                var pointGo = new GameObject("GoldenSplinePoint");
                pointGo.transform.SetParent(go.transform, false);
                pointGo.transform.position = point;
                pointGo.AddComponent<SplinePoint>();
                pointGo.AddComponent<SplinePointData>();
                pointGo.AddComponent<SplinePointDataWaves>().Weight = spline.waveMultiplier;
                pointGo.AddComponent<SplinePointDataFlow>().FlowVelocity = spline.flowSpeed;
                pointGo.AddComponent<SplinePointDataFoam>().FoamAmount = spline.foamAmount;
            }

            splineComponent.Radius = spline.width * 0.5f;
            splineComponent.Subdivisions = 8;

            go.AddComponent<RegisterFlowInput>();
            go.AddComponent<RegisterFoamInput>();

            var shape = go.AddComponent<ShapeFFT>();
            shape._spectrum = spectrum;
            shape._timeLoopLength = loopPeriod;

            splineComponent.UpdateSpline();
            return go;
        }

        static void BuildBoat(BoatSpec boat, ScenarioRig rig)
        {
            var go = GameObject.CreatePrimitive(PrimitiveType.Cube);
            go.name = "GoldenBoat";
            go.transform.position = boat.start;
            go.transform.rotation = Quaternion.Euler(0f, boat.headingDegrees, 0f);
            go.transform.localScale = boat.hullSize;
            rig.roots.Add(go);

            var body = go.AddComponent<Rigidbody>();
            body.mass = boat.mass;
            body.centerOfMass = Vector3.zero;
            rig.boatBody = body;

            var probes = go.AddComponent<BoatProbes>();
            probes._playerControlled = false;
            probes._minSpatialLength = boat.hullSize.x;
            rig.boatProbes = probes;

            var serialized = new SerializedObject(probes);
            var forcePoints = serialized.FindProperty("_forcePoints");
            if (forcePoints == null)
            {
                throw new InvalidOperationException("Crest BoatProbes._forcePoints not found; expected Crest 4.23.0");
            }
            forcePoints.arraySize = 4;
            var halfWidth = boat.hullSize.x * 0.5f;
            var halfLength = boat.hullSize.z * 0.5f;
            var offsets = new[]
            {
                new Vector3(-halfWidth, 0f, -halfLength),
                new Vector3(halfWidth, 0f, -halfLength),
                new Vector3(-halfWidth, 0f, halfLength),
                new Vector3(halfWidth, 0f, halfLength),
            };
            for (var i = 0; i < 4; i++)
            {
                var element = forcePoints.GetArrayElementAtIndex(i);
                element.FindPropertyRelative("_weight").floatValue = 1f;
                element.FindPropertyRelative("_offsetPosition").vector3Value = offsets[i];
            }
            serialized.ApplyModifiedProperties();

            var spheres = new List<SphereWaterInteraction>();
            foreach (var z in new[] { -boat.hullSize.z * 0.3f, boat.hullSize.z * 0.3f })
            {
                var sphereGo = new GameObject("GoldenHullSphere");
                sphereGo.transform.SetParent(go.transform, false);
                sphereGo.transform.localPosition = new Vector3(0f, 0f, z);
                var sphere = sphereGo.AddComponent<SphereWaterInteraction>();
                sphere._radius = 0.7f;
                sphere._weight = 2f;
                spheres.Add(sphere);
            }
            rig.hullSpheres = spheres.ToArray();

            var boatStart = typeof(BoatProbes).GetMethod("Start", BindingFlags.NonPublic | BindingFlags.Instance);
            rig.boatFixedUpdate = typeof(BoatProbes).GetMethod("FixedUpdate", BindingFlags.NonPublic | BindingFlags.Instance);
            rig.sphereLateUpdate = typeof(SphereWaterInteraction).GetMethod("LateUpdate", BindingFlags.NonPublic | BindingFlags.Instance);
            if (boatStart == null || rig.boatFixedUpdate == null || rig.sphereLateUpdate == null)
            {
                throw new InvalidOperationException("Crest boat/interaction callbacks not found; expected Crest 4.23.0");
            }
            boatStart.Invoke(probes, null);
        }
    }
}
