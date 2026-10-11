// HUtil32 相关静态方法的**逐字拷贝**（只适配外层类名/命名空间，函数体一行不改）。
// 每段标注来源行号；上游源码更新时必须同步。
// 来源：src/OpenMir2/HUtil32.cs（master f5170a8e）。

using System;

namespace EnvirGolden
{
    /// <summary>`HUtil32` 的 MapInfo/配置解析所需静态方法。</summary>
    public static class HUtil32
    {
        /// <summary>`src/OpenMir2/HUtil32.cs:9`。</summary>
        public const char Backslash = '/';

        /// <summary>`src/OpenMir2/HUtil32.cs:10`。</summary>
        public static readonly char[] Separator = { ' ', ',', '\t' };

        /// <summary>`src/OpenMir2/HUtil32.cs:299-357`（原文 private，此处保持函数体逐字）。</summary>
        private static string CaptureString(string source, ref string rdstr)
        {
            string result;
            int st;
            int et;
            if (string.IsNullOrEmpty(source))
            {
                return string.Empty;
            }
            int c = 1;
            int len = source.Length;
            while (source[c] == ' ')
            {
                if (c < len)
                {
                    c++;
                }
                else
                {
                    break;
                }
            }
            if (source[c] == '\"' && c < len)
            {
                st = c + 1;
                et = len;
                for (int i = c + 1; i <= len; i++)
                {
                    if (source[i] == '\"')
                    {
                        et = i - 1;
                        break;
                    }
                }
            }
            else
            {
                st = c;
                et = len;
                for (int i = c; i <= len; i++)
                {
                    if (source[i] == ' ')
                    {
                        et = i - 1;
                        break;
                    }
                }
            }
            rdstr = source.Substring(st - 1, et - st + 1);
            if (len >= et + 2)
            {
                result = source.Substring(et + 2 - 1, len - (et + 1));
            }
            else
            {
                result = string.Empty;
            }
            return result;
        }

        /// <summary>`src/OpenMir2/HUtil32.cs:359-362`。</summary>
        public static int StrToInt(string str, int def)
        {
            return int.TryParse(str, out int result) ? result : def;
        }

        /// <summary>`src/OpenMir2/HUtil32.cs:384-389`。</summary>
        public static string GetValidStr3(string source, ref string dest, char divider)
        {
            string[] ary = source.Split(divider, 2, StringSplitOptions.RemoveEmptyEntries); //返回不包含空的值
            dest = ary.Length > 0 ? ary[0] : string.Empty; //目标置为第一个
            return ary.Length > 1 ? ary[1] : string.Empty; //返回第二个
        }

        /// <summary>`src/OpenMir2/HUtil32.cs:391-403`。</summary>
        public static string GetValidStr3(string source, ref string dest, char[] dividerAry)
        {
            char[] div = new char[dividerAry.Length];
            for (int i = 0; i < dividerAry.Length; i++)
            {
                div[i] = dividerAry[i];
            }

            string[] ary = source.Split(div, 2, StringSplitOptions.RemoveEmptyEntries); //返回不包含空的值
            dest = ary.Length > 0 ? ary[0] : string.Empty; //目标置为第一个
            return ary.Length > 1 ? ary[1] : string.Empty; //返回第二个
        }

        /// <summary>`src/OpenMir2/HUtil32.cs:430-443`。</summary>
        public static string GetValidStrCap(string str, ref string dest, char divider)
        {
            string result;
            if (!string.IsNullOrEmpty(str))
            {
                result = str[0] == '\"' ? CaptureString(str, ref dest) : GetValidStr3(str, ref dest, divider);
            }
            else
            {
                result = string.Empty;
                dest = string.Empty;
            }
            return result;
        }

        /// <summary>`src/OpenMir2/HUtil32.cs:445-458`。</summary>
        public static string GetValidStrCap(string str, ref string dest, char[] divider)
        {
            string result;
            if (!string.IsNullOrEmpty(str))
            {
                result = str[0] == '\"' ? CaptureString(str, ref dest) : GetValidStr3(str, ref dest, divider);
            }
            else
            {
                result = string.Empty;
                dest = string.Empty;
            }
            return result;
        }

        /// <summary>`src/OpenMir2/HUtil32.cs:488-548`。</summary>
        public static string ArrestStringEx(string source, string searchAfter, string arrestBefore, ref string arrestStr)
        {
            if (string.IsNullOrEmpty(source))
            {
                return string.Empty;
            }
            ReadOnlySpan<char> sourceSpan = source.AsSpan();
            int spanLen = sourceSpan.Length;
            string result = string.Empty;
            bool findData = false;
            try
            {
                if (spanLen >= 2)
                {
                    if (source.StartsWith(searchAfter))
                    {
                        sourceSpan = sourceSpan[1..spanLen];
                        findData = true;
                    }
                    else
                    {
                        int n = sourceSpan.IndexOf(searchAfter, StringComparison.OrdinalIgnoreCase);
                        if (n > 0)
                        {
                            sourceSpan = sourceSpan.Slice(n + 1, spanLen - n - 1);
                            findData = true;
                        }
                    }
                }
                if (findData)
                {
                    int n = sourceSpan.IndexOf(arrestBefore, StringComparison.OrdinalIgnoreCase) + 1;
                    if (n > 0)
                    {
                        arrestStr = sourceSpan[..(n - 1)].ToString();
                        result = sourceSpan[(arrestStr.Length + 1)..].ToString();
                    }
                    else
                    {
                        result = searchAfter + sourceSpan.ToString();
                    }
                }
                else
                {
                    for (int i = 0; i < spanLen; i++)
                    {
                        if (sourceSpan[i - 1].ToString() == searchAfter)
                        {
                            result = sourceSpan.Slice(i - 1, spanLen - i + 1).ToString();
                            break;
                        }
                    }
                }
            }
            catch
            {
                arrestStr = string.Empty;
                result = string.Empty;
            }
            return result;
        }

        /// <summary>`src/OpenMir2/HUtil32.cs:550-578`。</summary>
        public static bool CompareLStr(string src, string targ)
        {
            int compn = targ.Length;
            if (compn <= 0)
            {
                return false;
            }

            if (src.Length < compn)
            {
                return false;
            }

            if (targ.Length < compn)
            {
                return false;
            }

            for (int i = 0; i < compn; i++)
            {
                if (char.ToUpper(src[i]) == char.ToUpper(targ[i]))
                {
                    continue;
                }

                return false;
            }
            return true;
        }
    }
}
