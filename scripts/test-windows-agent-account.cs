// 只管理 CI 临时账户的批处理登录权限，避免覆盖 runner 的安全策略。
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.Principal;

public static class NcdCiAccountRights
{
    [StructLayout(LayoutKind.Sequential)]
    private struct ObjectAttributes
    {
        public uint Length;
        public IntPtr RootDirectory, ObjectName;
        public uint Attributes;
        public IntPtr SecurityDescriptor, SecurityQualityOfService;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct UnicodeString
    {
        public ushort Length, MaximumLength;
        public IntPtr Buffer;
    }

    [DllImport("advapi32.dll", ExactSpelling = true)]
    private static extern uint LsaOpenPolicy(IntPtr systemName, ref ObjectAttributes attributes,
        uint access, out IntPtr policy);
    [DllImport("advapi32.dll", ExactSpelling = true)]
    private static extern uint LsaAddAccountRights(IntPtr policy, byte[] sid,
        UnicodeString[] rights, uint count);
    [DllImport("advapi32.dll", ExactSpelling = true)]
    private static extern uint LsaRemoveAccountRights(IntPtr policy, byte[] sid,
        [MarshalAs(UnmanagedType.U1)] bool allRights, UnicodeString[] rights, uint count);
    [DllImport("advapi32.dll", ExactSpelling = true)]
    private static extern uint LsaClose(IntPtr policy);
    [DllImport("advapi32.dll", ExactSpelling = true)]
    private static extern uint LsaNtStatusToWinError(uint status);

    private static void Check(uint status)
    {
        if (status != 0) throw new Win32Exception((int)LsaNtStatusToWinError(status));
    }

    public static void SetBatchLogon(SecurityIdentifier accountSid, bool enabled)
    {
        var sid = new byte[accountSid.BinaryLength];
        accountSid.GetBinaryForm(sid, 0);
        var attributes = new ObjectAttributes { Length = (uint)Marshal.SizeOf<ObjectAttributes>() };
        // POLICY_LOOKUP_NAMES | POLICY_CREATE_ACCOUNT
        Check(LsaOpenPolicy(IntPtr.Zero, ref attributes, 0x810, out var policy));
        var buffer = IntPtr.Zero;
        try
        {
            const string name = "SeBatchLogonRight";
            buffer = Marshal.StringToHGlobalUni(name);
            var rights = new[] { new UnicodeString {
                Length = (ushort)(name.Length * 2),
                MaximumLength = (ushort)((name.Length + 1) * 2), Buffer = buffer
            } };
            Check(enabled ? LsaAddAccountRights(policy, sid, rights, 1)
                : LsaRemoveAccountRights(policy, sid, false, rights, 1));
        }
        finally
        {
            if (buffer != IntPtr.Zero) Marshal.FreeHGlobal(buffer);
            LsaClose(policy);
        }
    }
}
