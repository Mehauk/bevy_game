using System.Collections.Generic;
using System.Globalization;
using System.Text;

namespace Crest.Goldens
{
    public sealed class SeabedDto
    {
        public string kind;
        public double height;
        public double baseHeight;
        public double slopeX;
        public double slopeZ;
        public double min;
        public double max;
    }

    public sealed class SplineDto
    {
        public List<double[]> points = new List<double[]>();
        public double width;
        public double flowSpeed;
        public double waveMultiplier;
        public double foamAmount;
    }

    public sealed class GoldenSettingsDto
    {
        public double waterLevel;
        public double windSpeed;
        public double windDirX;
        public double windDirZ;
        public double gravity;
        public double chop;
        public double turbulence;
        public double amplitudeMultiplier;
        public double loopPeriod;
        public int lodCount;
        public int lodResolution;
        public double baseTexelSize;
        public SeabedDto seabed;
        public List<SplineDto> splines = new List<SplineDto>();
    }

    public sealed class ProbeDto
    {
        public string name;
        public double x;
        public double z;
        public double t;
        public int lod;
        public double height;
        public double[] displacement;
        public double[] normal;
        public double[] flow;
        public double foam;
        public double seaFloorDepth;
    }

    public sealed class SliceDto
    {
        public string name;
        public int lod;
        public int t;
        public string channels;
        public int width;
        public int height;
        public string file;
    }

    public sealed class TrajectorySampleDto
    {
        public double t;
        public double[] position;
        public double[] rotationXyzw;
    }

    public sealed class TrajectoryDto
    {
        public string name;
        public List<TrajectorySampleDto> samples = new List<TrajectorySampleDto>();
    }

    public sealed class CameraDto
    {
        public double[] position;
        public double[] rotationXyzw;
        public double fov;
    }

    public sealed class FrameDto
    {
        public string name;
        public string file;
        public CameraDto camera;
    }

    public sealed class TolerancesDto
    {
        public double height;
        public double normal;
        public double flow;
        public double foam;
        public double depth;
        public double sliceRms;
        public double trajectory;
    }

    public sealed class GoldenDocumentDto
    {
        public string crestVersion = "4.23.0";
        public string coordinateSpace = "unity_left_handed";
        public GoldenSettingsDto settings;
        public List<ProbeDto> probes = new List<ProbeDto>();
        public List<SliceDto> slices = new List<SliceDto>();
        public List<TrajectoryDto> trajectories = new List<TrajectoryDto>();
        public List<FrameDto> frames = new List<FrameDto>();
        public TolerancesDto tolerances;
    }

    public static class GoldenJson
    {
        public static string Write(GoldenDocumentDto doc)
        {
            var sb = new StringBuilder();
            sb.Append('{');
            Key(sb, "crest_version"); StrVal(sb, doc.crestVersion); sb.Append(',');
            Key(sb, "coordinate_space"); StrVal(sb, doc.coordinateSpace); sb.Append(',');
            Key(sb, "settings"); WriteSettings(sb, doc.settings); sb.Append(',');
            Key(sb, "probes"); WriteArray(sb, doc.probes, WriteProbe); sb.Append(',');
            Key(sb, "slices"); WriteArray(sb, doc.slices, WriteSlice); sb.Append(',');
            Key(sb, "trajectories"); WriteArray(sb, doc.trajectories, WriteTrajectory); sb.Append(',');
            Key(sb, "frames"); WriteArray(sb, doc.frames, WriteFrame); sb.Append(',');
            Key(sb, "tolerances"); WriteTolerances(sb, doc.tolerances);
            sb.Append('}');
            return sb.ToString();
        }

        static void WriteSettings(StringBuilder sb, GoldenSettingsDto s)
        {
            sb.Append('{');
            Key(sb, "water_level"); NumVal(sb, s.waterLevel); sb.Append(',');
            Key(sb, "wind_speed"); NumVal(sb, s.windSpeed); sb.Append(',');
            Key(sb, "wind_direction"); sb.Append('['); NumVal(sb, s.windDirX); sb.Append(','); NumVal(sb, s.windDirZ); sb.Append(']'); sb.Append(',');
            Key(sb, "gravity"); NumVal(sb, s.gravity); sb.Append(',');
            Key(sb, "chop"); NumVal(sb, s.chop); sb.Append(',');
            Key(sb, "turbulence"); NumVal(sb, s.turbulence); sb.Append(',');
            Key(sb, "amplitude_multiplier"); NumVal(sb, s.amplitudeMultiplier); sb.Append(',');
            Key(sb, "loop_period"); NumVal(sb, s.loopPeriod); sb.Append(',');
            Key(sb, "lod_count"); NumVal(sb, s.lodCount); sb.Append(',');
            Key(sb, "lod_resolution"); NumVal(sb, s.lodResolution); sb.Append(',');
            Key(sb, "base_texel_size"); NumVal(sb, s.baseTexelSize); sb.Append(',');
            Key(sb, "seabed"); WriteSeabed(sb, s.seabed); sb.Append(',');
            Key(sb, "splines"); WriteArray(sb, s.splines, WriteSpline);
            sb.Append('}');
        }

        static void WriteSeabed(StringBuilder sb, SeabedDto s)
        {
            sb.Append('{');
            Key(sb, "kind"); StrVal(sb, s.kind);
            if (s.kind == "flat")
            {
                sb.Append(',');
                Key(sb, "height"); NumVal(sb, s.height);
            }
            else
            {
                sb.Append(',');
                Key(sb, "base"); NumVal(sb, s.baseHeight); sb.Append(',');
                Key(sb, "slope_x"); NumVal(sb, s.slopeX); sb.Append(',');
                Key(sb, "slope_z"); NumVal(sb, s.slopeZ); sb.Append(',');
                Key(sb, "min"); NumVal(sb, s.min); sb.Append(',');
                Key(sb, "max"); NumVal(sb, s.max);
            }
            sb.Append('}');
        }

        static void WriteSpline(StringBuilder sb, SplineDto s)
        {
            sb.Append('{');
            Key(sb, "points");
            sb.Append('[');
            for (var i = 0; i < s.points.Count; i++)
            {
                if (i > 0) sb.Append(',');
                Vec(sb, s.points[i]);
            }
            sb.Append(']'); sb.Append(',');
            Key(sb, "width"); NumVal(sb, s.width); sb.Append(',');
            Key(sb, "flow_speed"); NumVal(sb, s.flowSpeed); sb.Append(',');
            Key(sb, "wave_multiplier"); NumVal(sb, s.waveMultiplier); sb.Append(',');
            Key(sb, "foam_amount"); NumVal(sb, s.foamAmount);
            sb.Append('}');
        }

        static void WriteProbe(StringBuilder sb, ProbeDto p)
        {
            sb.Append('{');
            Key(sb, "name"); StrVal(sb, p.name); sb.Append(',');
            Key(sb, "x"); NumVal(sb, p.x); sb.Append(',');
            Key(sb, "z"); NumVal(sb, p.z); sb.Append(',');
            Key(sb, "t"); NumVal(sb, p.t); sb.Append(',');
            Key(sb, "lod"); NumVal(sb, p.lod); sb.Append(',');
            Key(sb, "height"); NumVal(sb, p.height); sb.Append(',');
            Key(sb, "displacement"); Vec(sb, p.displacement); sb.Append(',');
            Key(sb, "normal"); Vec(sb, p.normal); sb.Append(',');
            Key(sb, "flow"); Vec(sb, p.flow); sb.Append(',');
            Key(sb, "foam"); NumVal(sb, p.foam); sb.Append(',');
            Key(sb, "sea_floor_depth"); NumVal(sb, p.seaFloorDepth);
            sb.Append('}');
        }

        static void WriteSlice(StringBuilder sb, SliceDto s)
        {
            sb.Append('{');
            Key(sb, "name"); StrVal(sb, s.name); sb.Append(',');
            Key(sb, "lod"); NumVal(sb, s.lod); sb.Append(',');
            Key(sb, "t"); NumVal(sb, s.t); sb.Append(',');
            Key(sb, "channels"); StrVal(sb, s.channels); sb.Append(',');
            Key(sb, "width"); NumVal(sb, s.width); sb.Append(',');
            Key(sb, "height"); NumVal(sb, s.height); sb.Append(',');
            Key(sb, "file"); StrVal(sb, s.file);
            sb.Append('}');
        }

        static void WriteTrajectory(StringBuilder sb, TrajectoryDto t)
        {
            sb.Append('{');
            Key(sb, "name"); StrVal(sb, t.name); sb.Append(',');
            Key(sb, "samples");
            sb.Append('[');
            for (var i = 0; i < t.samples.Count; i++)
            {
                if (i > 0) sb.Append(',');
                var sample = t.samples[i];
                sb.Append('{');
                Key(sb, "t"); NumVal(sb, sample.t); sb.Append(',');
                Key(sb, "position"); Vec(sb, sample.position); sb.Append(',');
                Key(sb, "rotation_xyzw"); Vec(sb, sample.rotationXyzw);
                sb.Append('}');
            }
            sb.Append(']');
            sb.Append('}');
        }

        static void WriteFrame(StringBuilder sb, FrameDto f)
        {
            sb.Append('{');
            Key(sb, "name"); StrVal(sb, f.name); sb.Append(',');
            Key(sb, "file"); StrVal(sb, f.file); sb.Append(',');
            Key(sb, "camera");
            sb.Append('{');
            Key(sb, "position"); Vec(sb, f.camera.position); sb.Append(',');
            Key(sb, "rotation_xyzw"); Vec(sb, f.camera.rotationXyzw); sb.Append(',');
            Key(sb, "fov"); NumVal(sb, f.camera.fov);
            sb.Append('}');
            sb.Append('}');
        }

        static void WriteTolerances(StringBuilder sb, TolerancesDto t)
        {
            sb.Append('{');
            Key(sb, "height"); NumVal(sb, t.height); sb.Append(',');
            Key(sb, "normal"); NumVal(sb, t.normal); sb.Append(',');
            Key(sb, "flow"); NumVal(sb, t.flow); sb.Append(',');
            Key(sb, "foam"); NumVal(sb, t.foam); sb.Append(',');
            Key(sb, "depth"); NumVal(sb, t.depth); sb.Append(',');
            Key(sb, "slice_rms"); NumVal(sb, t.sliceRms); sb.Append(',');
            Key(sb, "trajectory"); NumVal(sb, t.trajectory);
            sb.Append('}');
        }

        static void WriteArray<T>(StringBuilder sb, List<T> items, System.Action<StringBuilder, T> writeItem)
        {
            sb.Append('[');
            for (var i = 0; i < items.Count; i++)
            {
                if (i > 0) sb.Append(',');
                writeItem(sb, items[i]);
            }
            sb.Append(']');
        }

        static void Key(StringBuilder sb, string key)
        {
            sb.Append('"').Append(key).Append("\":");
        }

        static void StrVal(StringBuilder sb, string value)
        {
            sb.Append('"').Append(Escape(value)).Append('"');
        }

        static void NumVal(StringBuilder sb, double value)
        {
            sb.Append(value.ToString("R", CultureInfo.InvariantCulture));
        }

        static void NumVal(StringBuilder sb, int value)
        {
            sb.Append(value.ToString(CultureInfo.InvariantCulture));
        }

        static void Vec(StringBuilder sb, double[] values)
        {
            sb.Append('[');
            for (var i = 0; i < values.Length; i++)
            {
                if (i > 0) sb.Append(',');
                NumVal(sb, values[i]);
            }
            sb.Append(']');
        }

        static string Escape(string value)
        {
            if (string.IsNullOrEmpty(value)) return string.Empty;
            var sb = new StringBuilder(value.Length + 8);
            foreach (var c in value)
            {
                switch (c)
                {
                    case '"': sb.Append("\\\""); break;
                    case '\\': sb.Append("\\\\"); break;
                    case '\n': sb.Append("\\n"); break;
                    case '\r': sb.Append("\\r"); break;
                    case '\t': sb.Append("\\t"); break;
                    default:
                        if (c < 0x20) sb.Append("\\u").Append(((int)c).ToString("x4", CultureInfo.InvariantCulture));
                        else sb.Append(c);
                        break;
                }
            }
            return sb.ToString();
        }
    }
}
