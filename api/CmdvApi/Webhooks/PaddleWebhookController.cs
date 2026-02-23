using System.Text.Json;
using CmdvApi.Services;
using Microsoft.AspNetCore.Mvc;

namespace CmdvApi.Webhooks;

[ApiController]
[Route("webhooks/paddle")]
public class PaddleWebhookController : ControllerBase
{
    private readonly PaddleService _paddle;

    public PaddleWebhookController(PaddleService paddle)
    {
        _paddle = paddle;
    }

    [HttpPost]
    public async Task<IActionResult> HandleWebhook()
    {
        using var reader = new StreamReader(Request.Body);
        var payload = await reader.ReadToEndAsync();

        var signature = Request.Headers["Paddle-Signature"].FirstOrDefault();
        if (string.IsNullOrEmpty(signature) || !_paddle.VerifySignature(payload, signature))
            return Unauthorized();

        var doc = JsonDocument.Parse(payload);
        var root = doc.RootElement;
        var eventType = root.GetProperty("event_type").GetString();

        switch (eventType)
        {
            case "subscription.created":
            {
                var data = root.GetProperty("data");
                var customerId = data.GetProperty("customer_id").GetString()!;
                var subscriptionId = data.GetProperty("id").GetString()!;
                var nextBilled = data.GetProperty("next_billed_at").GetDateTime();
                await _paddle.HandleSubscriptionCreated(customerId, subscriptionId, nextBilled);
                break;
            }
            case "subscription.canceled":
            {
                var data = root.GetProperty("data");
                var subscriptionId = data.GetProperty("id").GetString()!;
                await _paddle.HandleSubscriptionCancelled(subscriptionId);
                break;
            }
            case "subscription.updated":
            {
                var data = root.GetProperty("data");
                var subscriptionId = data.GetProperty("id").GetString()!;
                var nextBilled = data.GetProperty("next_billed_at").GetDateTime();
                await _paddle.HandleSubscriptionUpdated(subscriptionId, nextBilled);
                break;
            }
        }

        return Ok();
    }
}
