using System.Runtime.InteropServices;

namespace SiftX;

internal sealed class SafeFileHandle : SafeHandle
{
    public SafeFileHandle() : base(0, ownsHandle: true) { }

    public SafeFileHandle(nint handle) : base(handle, ownsHandle: true) { }

    public override bool IsInvalid => handle == 0;

    protected override bool ReleaseHandle()
    {
        Native.FileFree(handle);
        return true;
    }
}

internal sealed class SafeDocumentHandle : SafeHandle
{
    public SafeDocumentHandle() : base(0, ownsHandle: true) { }

    public SafeDocumentHandle(nint handle) : base(handle, ownsHandle: true) { }

    public override bool IsInvalid => handle == 0;

    protected override bool ReleaseHandle()
    {
        Native.DocumentFree(handle);
        return true;
    }
}

internal sealed class SafeTagArrayHandle : SafeHandle
{
    public SafeTagArrayHandle() : base(0, ownsHandle: true) { }

    public SafeTagArrayHandle(nint handle) : base(handle, ownsHandle: true) { }

    public override bool IsInvalid => handle == 0;

    protected override bool ReleaseHandle()
    {
        Native.TagsFree(handle);
        return true;
    }
}

internal sealed class SafeImageArrayHandle : SafeHandle
{
    public SafeImageArrayHandle() : base(0, ownsHandle: true) { }

    public SafeImageArrayHandle(nint handle) : base(handle, ownsHandle: true) { }

    public override bool IsInvalid => handle == 0;

    protected override bool ReleaseHandle()
    {
        Native.ImagesFree(handle);
        return true;
    }
}

internal sealed class SafeTextPagesHandle : SafeHandle
{
    public SafeTextPagesHandle() : base(0, ownsHandle: true) { }

    public SafeTextPagesHandle(nint handle) : base(handle, ownsHandle: true) { }

    public override bool IsInvalid => handle == 0;

    protected override bool ReleaseHandle()
    {
        Native.TextPagesFree(handle);
        return true;
    }
}

internal sealed class SafeFormFieldArrayHandle : SafeHandle
{
    public SafeFormFieldArrayHandle() : base(0, ownsHandle: true) { }

    public SafeFormFieldArrayHandle(nint handle) : base(handle, ownsHandle: true) { }

    public override bool IsInvalid => handle == 0;

    protected override bool ReleaseHandle()
    {
        Native.FormFieldsFree(handle);
        return true;
    }
}

internal sealed class SafeAnnotationArrayHandle : SafeHandle
{
    public SafeAnnotationArrayHandle() : base(0, ownsHandle: true) { }

    public SafeAnnotationArrayHandle(nint handle) : base(handle, ownsHandle: true) { }

    public override bool IsInvalid => handle == 0;

    protected override bool ReleaseHandle()
    {
        Native.AnnotationsFree(handle);
        return true;
    }
}

internal sealed class SafeStructTreeArrayHandle : SafeHandle
{
    public SafeStructTreeArrayHandle() : base(0, ownsHandle: true) { }

    public SafeStructTreeArrayHandle(nint handle) : base(handle, ownsHandle: true) { }

    public override bool IsInvalid => handle == 0;

    protected override bool ReleaseHandle()
    {
        Native.StructTreeFree(handle);
        return true;
    }
}
