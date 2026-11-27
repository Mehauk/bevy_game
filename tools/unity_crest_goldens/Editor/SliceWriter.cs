using System;
using System.IO;
using UnityEngine;
using UnityEngine.Rendering;

namespace Crest.Goldens
{
    public enum SliceChannel
    {
        DisplacementXyzJac,
        FlowXz,
        Foam,
        SeaFloorDepth
    }

    public sealed class SliceCapture
    {
        public SliceChannel channel;
        public string sliceName;
        public string fileName;
        public int lod;
        public int width;
        public int height;
        public int channelsPerPixel;
        public bool halfPrecision;
        public float texelWidth;
        public float seaLevel;
        public AsyncGPUReadbackRequest request;
    }

    public static class SliceWriter
    {
        public static SliceCapture Begin(LodDataMgr manager, int lod, SliceChannel channel, string sliceName, string fileName)
        {
            if (manager == null)
            {
                throw new InvalidOperationException("Crest simulation for slice '" + sliceName + "' is not active");
            }

            var renderData = OceanRenderer.Instance._lodTransform._renderData[lod].Current;
            var format = manager.CompatibleTextureFormat;
            var formatName = format.ToString();

            var channels = 0;
            foreach (var c in formatName)
            {
                if (c == 'R' || c == 'G' || c == 'B' || c == 'A') channels++;
            }

            return new SliceCapture
            {
                channel = channel,
                sliceName = sliceName,
                fileName = fileName,
                lod = lod,
                width = (int)renderData._textureRes,
                height = (int)renderData._textureRes,
                channelsPerPixel = channels,
                halfPrecision = formatName.StartsWith("R16"),
                texelWidth = renderData._texelWidth,
                seaLevel = OceanRenderer.Instance.SeaLevel,
                request = AsyncGPUReadback.Request(manager.DataTexture, lod, 0, format, (Action<AsyncGPUReadbackRequest>)null)
            };
        }

        public static bool IsDone(SliceCapture capture)
        {
            return capture.request.done;
        }

        public static float[] Extract(SliceCapture capture)
        {
            if (!capture.request.done)
            {
                throw new InvalidOperationException("readback not finished for '" + capture.sliceName + "'");
            }
            if (capture.request.hasError)
            {
                throw new InvalidOperationException("readback failed for '" + capture.sliceName + "'");
            }

            var pixelCount = capture.width * capture.height;
            var expectedChannels = ExpectedChannels(capture.channel);
            if (capture.channelsPerPixel != expectedChannels)
            {
                throw new InvalidOperationException("data texture format for '" + capture.sliceName + "' has " + capture.channelsPerPixel + " channels, expected " + expectedChannels);
            }
            float[] raw;
            if (capture.halfPrecision)
            {
                var halves = capture.request.GetData<ushort>().ToArray();
                raw = new float[halves.Length];
                for (var i = 0; i < halves.Length; i++)
                {
                    raw[i] = HalfToFloat(halves[i]);
                }
            }
            else
            {
                raw = capture.request.GetData<float>().ToArray();
            }

            switch (capture.channel)
            {
                case SliceChannel.DisplacementXyzJac: return WithJacobian(raw, capture.width, capture.height, capture.texelWidth);
                case SliceChannel.FlowXz: return raw;
                case SliceChannel.Foam: return raw;
                case SliceChannel.SeaFloorDepth: return ToDepth(raw, pixelCount, capture.seaLevel);
                default: throw new InvalidOperationException("unknown slice channel");
            }
        }

        public static float SamplePoint(SliceCapture capture, float[] data, int col, int row)
        {
            var index = row * capture.width + col;
            switch (capture.channel)
            {
                case SliceChannel.Foam: return data[index];
                case SliceChannel.SeaFloorDepth: return data[index];
                default: throw new InvalidOperationException("channel is not point-samplable");
            }
        }

        static int ExpectedChannels(SliceChannel channel)
        {
            switch (channel)
            {
                case SliceChannel.DisplacementXyzJac: return 4;
                case SliceChannel.FlowXz: return 2;
                case SliceChannel.Foam: return 1;
                case SliceChannel.SeaFloorDepth: return 2;
                default: throw new InvalidOperationException("unknown slice channel");
            }
        }

        public static string ChannelsLabel(SliceChannel channel)
        {
            switch (channel)
            {
                case SliceChannel.DisplacementXyzJac: return "disp_xyz_jac";
                case SliceChannel.FlowXz: return "flow_xz";
                case SliceChannel.Foam: return "foam";
                case SliceChannel.SeaFloorDepth: return "depth";
                default: throw new InvalidOperationException("unknown slice channel");
            }
        }

        public static void WriteBin(string path, float[] data)
        {
            using (var stream = File.Open(path, FileMode.Create, FileAccess.Write))
            using (var writer = new BinaryWriter(stream))
            {
                for (var i = 0; i < data.Length; i++)
                {
                    writer.Write(data[i]);
                }
            }
        }

        static float[] WithJacobian(float[] raw, int width, int height, float texelWidth)
        {
            var output = new float[raw.Length];
            for (var row = 0; row < height; row++)
            {
                for (var col = 0; col < width; col++)
                {
                    var i = row * width + col;
                    var x = raw[i * 4];
                    var y = raw[i * 4 + 1];
                    var z = raw[i * 4 + 2];
                    var right = col < width - 1 ? i + 1 : i;
                    var up = row < height - 1 ? i + width : i;
                    var dxX = (raw[right * 4] - x) / texelWidth;
                    var dxZ = (raw[right * 4 + 2] - z) / texelWidth;
                    var dzX = (raw[up * 4] - x) / texelWidth;
                    var dzZ = (raw[up * 4 + 2] - z) / texelWidth;
                    output[i * 4] = x;
                    output[i * 4 + 1] = y;
                    output[i * 4 + 2] = z;
                    output[i * 4 + 3] = dxX * dzZ - dxZ * dzX;
                }
            }
            return output;
        }

        static float[] ToDepth(float[] raw, int pixelCount, float seaLevel)
        {
            var output = new float[pixelCount];
            for (var i = 0; i < pixelCount; i++)
            {
                output[i] = seaLevel + raw[i * 2 + 1] - raw[i * 2];
            }
            return output;
        }

        static float HalfToFloat(ushort value)
        {
            var sign = (uint)value & 0x8000u;
            var exponent = ((uint)value >> 10) & 0x1Fu;
            var mantissa = (uint)value & 0x3FFu;
            uint bits;
            if (exponent == 0u)
            {
                if (mantissa == 0u)
                {
                    bits = sign;
                }
                else
                {
                    var e = 127u - 15u + 1u;
                    var m = mantissa;
                    while ((m & 0x400u) == 0u)
                    {
                        m <<= 1;
                        e--;
                    }
                    m &= 0x3FFu;
                    bits = sign | (e << 23) | (m << 13);
                }
            }
            else if (exponent == 31u)
            {
                bits = sign | 0x7F800000u | (mantissa << 13);
            }
            else
            {
                bits = sign | ((exponent + 127u - 15u) << 23) | (mantissa << 13);
            }
            return BitConverter.ToSingle(BitConverter.GetBytes(bits), 0);
        }
    }
}
