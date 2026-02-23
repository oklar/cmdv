using Amazon.S3;
using Amazon.S3.Model;

namespace CmdvApi.Services;

public class R2Service
{
    private readonly IAmazonS3 _s3;
    private readonly string _bucketName;
    private const int UrlExpirySecs = 60;
    private const long MaxContentLength = 55 * 1024 * 1024;

    public R2Service(IAmazonS3 s3, IConfiguration config)
    {
        _s3 = s3;
        _bucketName = config["R2:BucketName"] ?? "cmdv-sync";
    }

    public string GenerateDownloadUrl(int userId)
    {
        var request = new GetPreSignedUrlRequest
        {
            BucketName = _bucketName,
            Key = $"blobs/{userId}.enc",
            Verb = HttpVerb.GET,
            Expires = DateTime.UtcNow.AddSeconds(UrlExpirySecs),
        };

        return _s3.GetPreSignedURL(request);
    }

    public string GenerateUploadUrl(int userId)
    {
        var request = new GetPreSignedUrlRequest
        {
            BucketName = _bucketName,
            Key = $"blobs/{userId}.enc",
            Verb = HttpVerb.PUT,
            Expires = DateTime.UtcNow.AddSeconds(UrlExpirySecs),
            ContentType = "application/octet-stream",
        };

        request.Headers["x-amz-content-length-range"] = $"0,{MaxContentLength}";

        return _s3.GetPreSignedURL(request);
    }

    public async Task DeleteBlob(int userId)
    {
        await _s3.DeleteObjectAsync(_bucketName, $"blobs/{userId}.enc");
    }
}
